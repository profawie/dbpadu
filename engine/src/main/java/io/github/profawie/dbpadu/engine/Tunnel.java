package io.github.profawie.dbpadu.engine;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.jcraft.jsch.ChannelDirectTCPIP;
import com.jcraft.jsch.JSch;
import com.jcraft.jsch.JSchException;
import com.jcraft.jsch.Proxy;
import com.jcraft.jsch.SocketFactory;
import com.jcraft.jsch.UserInfo;
import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.ConnectException;
import java.net.NoRouteToHostException;
import java.net.Socket;
import java.net.SocketTimeoutException;
import java.net.UnknownHostException;
import java.nio.charset.StandardCharsets;
import java.sql.SQLException;
import java.util.ArrayList;
import java.util.List;

/**
 * An SSH tunnel: a port on this computer that leads, through an SSH
 * server, to the database. The SSH server itself may be behind one or
 * more jump servers (what OpenSSH calls ProxyJump).
 */
final class Tunnel implements AutoCloseable {
    /** Stands for the tunnel's port in the address the app sends. */
    static final String PORT_PLACEHOLDER = "${TUNNEL_PORT}";

    /** Outermost jump server first, the SSH server last. */
    private final List<com.jcraft.jsch.Session> sessions;
    final int localPort;

    private Tunnel(List<com.jcraft.jsch.Session> sessions, int localPort) {
        this.sessions = sessions;
        this.localPort = localPort;
    }

    /** One SSH server on the way: a jump server, or the last one. */
    private record Hop(String host, int port, String user, String keyFile, String secret) {
        static Hop of(JsonObject object) {
            return new Hop(
                text(object, "host"),
                object.has("port") ? object.get("port").getAsInt() : 22,
                text(object, "user"),
                text(object, "keyFile"),
                text(object, "secret"));
        }

        String name() {
            return port == 22 ? host : host + ":" + port;
        }
    }

    /**
     * Sign in at each jump server in order, then at the SSH server, and
     * open the tunnel to the database from there.
     *
     * The first server is reached directly. Each one after it is reached
     * through a channel on the one before, so what is sent to it is
     * encrypted end to end and the jump servers cannot read it.
     */
    static Tunnel open(JsonObject ssh) throws SQLException {
        List<Hop> hops = new ArrayList<>();
        if (ssh.has("jumps") && ssh.get("jumps").isJsonArray()) {
            for (JsonElement jump : ssh.getAsJsonArray("jumps")) {
                hops.add(Hop.of(jump.getAsJsonObject()));
            }
        }
        hops.add(Hop.of(ssh));
        String knownHosts = text(ssh, "knownHosts");
        List<com.jcraft.jsch.Session> sessions = new ArrayList<>();
        try {
            StringBuilder route = new StringBuilder();
            for (Hop hop : hops) {
                Hop via = sessions.isEmpty() ? null : hops.get(sessions.size() - 1);
                com.jcraft.jsch.Session before = sessions.isEmpty() ? null : sessions.get(sessions.size() - 1);
                try {
                    com.jcraft.jsch.Session session = connect(hop, before, knownHosts);
                    sessions.add(session);
                    route.append(route.isEmpty() ? "" : " -> ")
                        .append(hop.user()).append('@').append(hop.name())
                        .append(" (server key ").append(session.getHostKey().getFingerPrint(new JSch())).append(')');
                } catch (JSchException | IOException problem) {
                    throw new SQLException("SSH: " + explain(problem, hop, via), problem);
                }
            }
            com.jcraft.jsch.Session last = sessions.get(sessions.size() - 1);
            int local;
            try {
                local = last.setPortForwardingL(
                    "127.0.0.1", 0, text(ssh, "remoteHost"), ssh.get("remotePort").getAsInt());
            } catch (JSchException problem) {
                throw new SQLException("SSH: " + problem.getMessage(), problem);
            }
            System.err.println("[engine] SSH tunnel through " + route + " on local port " + local);
            return new Tunnel(sessions, local);
        } catch (SQLException | RuntimeException problem) {
            disconnect(sessions);
            throw problem;
        }
    }

    private static com.jcraft.jsch.Session connect(Hop hop, com.jcraft.jsch.Session before, String knownHosts)
            throws JSchException, IOException {
        // Each server gets its own JSch, so a key is only ever offered to
        // the server it was chosen for.
        JSch jsch = new JSch();
        if (!knownHosts.isEmpty()) {
            File file = new File(knownHosts);
            if (file.getParentFile() != null) {
                file.getParentFile().mkdirs();
            }
            file.createNewFile();
            jsch.setKnownHosts(file.getAbsolutePath());
        }
        boolean withKey = !hop.keyFile().isEmpty();
        if (withKey) {
            byte[] passphrase = hop.secret().isEmpty() ? null : hop.secret().getBytes(StandardCharsets.UTF_8);
            jsch.addIdentity(hop.keyFile(), passphrase);
        }
        com.jcraft.jsch.Session session = jsch.getSession(hop.user(), hop.host(), hop.port());
        if (!withKey) {
            session.setPassword(hop.secret());
        }
        if (before != null) {
            session.setProxy(new Through(before));
        }
        session.setConfig("PreferredAuthentications", withKey ? "publickey" : "password");
        // A server seen for the first time is trusted and remembered in
        // the known hosts file. A server whose key has changed since
        // then is refused.
        session.setConfig("StrictHostKeyChecking", "ask");
        session.setUserInfo(new FirstUse());
        session.setServerAliveInterval(30_000);
        session.connect(15_000);
        return session;
    }

    /** Says what went wrong at `hop`, reached through `via` (null: directly). */
    private static String explain(Exception problem, Hop hop, Hop via) {
        String message = String.valueOf(problem.getMessage());
        String where = hop.name();
        if (message.contains("HostKey has been changed") || message.contains("reject HostKey")) {
            return "the key of " + where + " is not the one seen before. Connecting was stopped, "
                + "because this can mean someone is in between. If the server was reinstalled, remove "
                + "its line from the known_hosts file in dbpadu's data folder.";
        }
        if (message.contains("Auth fail") || message.contains("Auth cancel")) {
            return where + " did not accept the SSH user name, password or key.";
        }
        if (via != null && message.contains("channel is not opened")) {
            return via.name() + " could not open a connection to " + hop.host() + ":" + hop.port()
                + ". The jump server connects to this address itself, so it must be an address and "
                + "port that work from " + via.name() + ", and the jump server must allow forwarding "
                + "(AllowTcpForwarding in its SSH server settings).";
        }
        if (via == null && (problem.getCause() instanceof ConnectException
                || problem.getCause() instanceof NoRouteToHostException
                || problem.getCause() instanceof SocketTimeoutException
                || message.contains("timeout"))) {
            return "this computer could not reach " + hop.host() + ":" + hop.port() + " (" + message
                + "). If that server is only reachable through another one, add that one as a "
                + "jump server on the SSH page of the connection.";
        }
        if (problem.getCause() instanceof UnknownHostException) {
            return "the name " + hop.host() + " was not found.";
        }
        return where + ": " + message;
    }

    private static void disconnect(List<com.jcraft.jsch.Session> sessions) {
        for (int index = sessions.size() - 1; index >= 0; index--) {
            sessions.get(index).disconnect();
        }
    }

    @Override
    public void close() {
        disconnect(sessions);
    }

    /** Reaches the next server through a channel on the one before. */
    private static final class Through implements Proxy {
        private final com.jcraft.jsch.Session before;
        private ChannelDirectTCPIP channel;
        private InputStream in;
        private OutputStream out;

        Through(com.jcraft.jsch.Session before) {
            this.before = before;
        }

        @Override
        public void connect(SocketFactory factory, String host, int port, int timeout) throws Exception {
            channel = (ChannelDirectTCPIP) before.getStreamForwarder(host, port);
            in = channel.getInputStream();
            out = channel.getOutputStream();
            channel.connect(timeout);
        }

        @Override
        public InputStream getInputStream() {
            return in;
        }

        @Override
        public OutputStream getOutputStream() {
            return out;
        }

        @Override
        public Socket getSocket() {
            return null;
        }

        @Override
        public void close() {
            if (channel != null) {
                channel.disconnect();
            }
        }
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
