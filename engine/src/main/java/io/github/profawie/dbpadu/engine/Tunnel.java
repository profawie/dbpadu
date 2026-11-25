package io.github.profawie.dbpadu.engine;

import com.google.gson.JsonObject;
import com.jcraft.jsch.JSch;
import com.jcraft.jsch.JSchException;
import com.jcraft.jsch.UserInfo;
import java.io.File;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.sql.SQLException;

/**
 * An SSH tunnel: a port on this computer that leads, through an SSH
 * server, to the database.
 */
final class Tunnel implements AutoCloseable {
    /** Stands for the tunnel's port in the address the app sends. */
    static final String PORT_PLACEHOLDER = "${TUNNEL_PORT}";

    private final com.jcraft.jsch.Session session;
    final int localPort;

    private Tunnel(com.jcraft.jsch.Session session, int localPort) {
        this.session = session;
        this.localPort = localPort;
    }

    static Tunnel open(JsonObject ssh) throws SQLException {
        String host = text(ssh, "host");
        int port = ssh.has("port") ? ssh.get("port").getAsInt() : 22;
        String user = text(ssh, "user");
        String keyFile = text(ssh, "keyFile");
        String secret = text(ssh, "secret");
        String knownHosts = text(ssh, "knownHosts");
        try {
            JSch jsch = new JSch();
            if (!knownHosts.isEmpty()) {
                File file = new File(knownHosts);
                if (file.getParentFile() != null) {
                    file.getParentFile().mkdirs();
                }
                file.createNewFile();
                jsch.setKnownHosts(file.getAbsolutePath());
            }
            boolean withKey = !keyFile.isEmpty();
            if (withKey) {
                byte[] passphrase = secret.isEmpty() ? null : secret.getBytes(StandardCharsets.UTF_8);
                jsch.addIdentity(keyFile, passphrase);
            }
            com.jcraft.jsch.Session session = jsch.getSession(user, host, port);
            if (!withKey) {
                session.setPassword(secret);
            }
            session.setConfig("PreferredAuthentications", withKey ? "publickey" : "password");
            // A server seen for the first time is trusted and remembered in
            // the known hosts file. A server whose key has changed since
            // then is refused.
            session.setConfig("StrictHostKeyChecking", "ask");
            session.setUserInfo(new FirstUse());
            session.setServerAliveInterval(30_000);
            session.connect(15_000);
            int local = session.setPortForwardingL(
                "127.0.0.1", 0, text(ssh, "remoteHost"), ssh.get("remotePort").getAsInt());
            System.err.println(
                "[engine] SSH tunnel through " + user + "@" + host + ":" + port
                    + " on local port " + local + ", server key " + session.getHostKey().getFingerPrint(jsch));
            return new Tunnel(session, local);
        } catch (JSchException | IOException problem) {
            throw new SQLException("SSH: " + explain(problem), problem);
        }
    }

    private static String explain(Exception problem) {
        String message = String.valueOf(problem.getMessage());
        if (message.contains("HostKey has been changed") || message.contains("reject HostKey")) {
            return "the server's key is not the one seen before. Connecting was stopped, because "
                + "this can mean someone is in between. If the server was reinstalled, remove its "
                + "line from the known_hosts file in dbpadu's data folder.";
        }
        if (message.contains("Auth fail") || message.contains("Auth cancel")) {
            return "the SSH user name, password or key was not accepted.";
        }
        return message;
    }

    @Override
    public void close() {
        session.disconnect();
    }

    private static String text(JsonObject object, String name) {
        return object.has(name) && !object.get(name).isJsonNull() ? object.get(name).getAsString() : "";
    }

    /** Answers JSch's questions about server keys. */
    private static final class FirstUse implements UserInfo {
        @Override
        public boolean promptYesNo(String message) {
            // Yes to a new server and to creating the known hosts file;
            // no to replacing a key that changed.
            return !message.contains("CHANGED");
        }

        @Override
        public String getPassphrase() {
            return null;
        }

        @Override
        public String getPassword() {
            return null;
        }

        @Override
        public boolean promptPassword(String message) {
            return false;
        }

        @Override
        public boolean promptPassphrase(String message) {
            return false;
        }

        @Override
        public void showMessage(String message) {
            System.err.println("[ssh] " + message);
        }
    }
}
