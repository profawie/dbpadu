package io.github.profawie.dbpadu.engine;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.sql.SQLException;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicInteger;
import org.apache.sshd.common.session.Session;
import org.apache.sshd.common.util.net.SshdSocketAddress;
import org.apache.sshd.server.SshServer;
import org.apache.sshd.server.forward.AcceptAllForwardingFilter;
import org.apache.sshd.server.keyprovider.SimpleGeneratorHostKeyProvider;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

/** SSH tunnels against SSH servers that run inside the test. */
class TunnelTest {
    @TempDir
    Path folder;

    private final List<SshServer> servers = new ArrayList<>();
    private ServerSocket database;

    /** Stands for the database: answers every connection with one line. */
    @BeforeEach
    void startDatabase() throws IOException {
        database = new ServerSocket(0, 10, InetAddress.getLoopbackAddress());
        Thread thread = new Thread(() -> {
            while (!database.isClosed()) {
                try (Socket socket = database.accept()) {
                    OutputStream out = socket.getOutputStream();
                    out.write("database\n".getBytes(StandardCharsets.UTF_8));
                    out.flush();
                } catch (IOException closed) {
                    return;
                }
            }
        });
        thread.setDaemon(true);
        thread.start();
    }

    @AfterEach
    void stop() throws IOException {
        for (SshServer server : servers) {
            server.stop(true);
        }
        database.close();
    }

    /** An SSH server for `user` with `password`; counts what it forwards. */
    private int server(String user, String password, AtomicInteger forwards, boolean allow) throws IOException {
        SshServer server = SshServer.setUpDefaultServer();
        server.setHost("127.0.0.1");
        server.setPort(0);
        server.setKeyPairProvider(
            new SimpleGeneratorHostKeyProvider(folder.resolve("hostkey-" + servers.size())));
        server.setPasswordAuthenticator((name, typed, session) -> name.equals(user) && typed.equals(password));
        server.setForwardingFilter(new AcceptAllForwardingFilter() {
            @Override
            protected boolean checkAcceptance(String request, Session session, SshdSocketAddress target) {
                forwards.incrementAndGet();
                return allow;
            }
        });
        server.start();
        servers.add(server);
        return server.getPort();
    }

    private JsonObject hop(String user, String password, int port) {
        JsonObject hop = new JsonObject();
        hop.addProperty("host", "127.0.0.1");
        hop.addProperty("port", port);
        hop.addProperty("user", user);
        hop.addProperty("keyFile", "");
        hop.addProperty("secret", password);
        return hop;
    }

    private JsonObject target(JsonObject server, JsonObject... jumps) {
        server.addProperty("knownHosts", folder.resolve("known_hosts").toString());
        server.addProperty("remoteHost", "127.0.0.1");
        server.addProperty("remotePort", database.getLocalPort());
        JsonArray list = new JsonArray();
        for (JsonObject jump : jumps) {
            list.add(jump);
        }
        server.add("jumps", list);
        return server;
    }

    private static String greeting(Tunnel tunnel) throws IOException {
        try (Socket socket = new Socket("127.0.0.1", tunnel.localPort)) {
            InputStream in = socket.getInputStream();
            return new String(in.readAllBytes(), StandardCharsets.UTF_8).trim();
        }
    }

    @Test
    void theDatabaseIsReachedThroughOneServer() throws Exception {
        AtomicInteger forwards = new AtomicInteger();
        int port = server("me", "secret", forwards, true);
        try (Tunnel tunnel = Tunnel.open(target(hop("me", "secret", port)))) {
            assertEquals("database", greeting(tunnel));
        }
        assertEquals(1, forwards.get());
    }

    @Test
    void theDatabaseIsReachedThroughTwoJumpServers() throws Exception {
        AtomicInteger first = new AtomicInteger();
        AtomicInteger second = new AtomicInteger();
        AtomicInteger last = new AtomicInteger();
        int firstPort = server("one", "pw1", first, true);
        int secondPort = server("two", "pw2", second, true);
        int lastPort = server("three", "pw3", last, true);
        JsonObject target = target(
            hop("three", "pw3", lastPort), hop("one", "pw1", firstPort), hop("two", "pw2", secondPort));
        try (Tunnel tunnel = Tunnel.open(target)) {
            assertEquals("database", greeting(tunnel));
        }
        // Each server passed on exactly one connection: to the next server,
        // and the last one to the database.
        assertEquals(1, first.get());
        assertEquals(1, second.get());
        assertEquals(1, last.get());
        // Every server on the way is remembered.
        assertEquals(3, Files.readAllLines(folder.resolve("known_hosts")).size());
    }

    @Test
    void aWrongPasswordAtAJumpServerNamesThatServer() throws Exception {
        int jump = server("one", "pw1", new AtomicInteger(), true);
        int last = server("two", "pw2", new AtomicInteger(), true);
        JsonObject target = target(hop("two", "pw2", last), hop("one", "wrong", jump));
        SQLException problem = assertThrows(SQLException.class, () -> Tunnel.open(target));
        assertEquals(
            "SSH: 127.0.0.1:" + jump + " did not accept the SSH user name, password or key.",
            problem.getMessage());
    }

    @Test
    void aJumpServerThatDoesNotForwardIsNamed() throws Exception {
        int jump = server("one", "pw1", new AtomicInteger(), false);
        int last = server("two", "pw2", new AtomicInteger(), true);
        JsonObject target = target(hop("two", "pw2", last), hop("one", "pw1", jump));
        SQLException problem = assertThrows(SQLException.class, () -> Tunnel.open(target));
        assertTrue(
            problem.getMessage().startsWith(
                "SSH: 127.0.0.1:" + jump + " could not open a connection to 127.0.0.1:" + last),
            problem.getMessage());
    }

    @Test
    void aServerThatCannotBeReachedSuggestsAJumpServer() throws Exception {
        int closed;
        try (ServerSocket socket = new ServerSocket(0)) {
            closed = socket.getLocalPort();
        }
        JsonObject target = target(hop("me", "secret", closed));
        SQLException problem = assertThrows(SQLException.class, () -> Tunnel.open(target));
        assertTrue(problem.getMessage().contains("add that one as a jump server"), problem.getMessage());
    }
}
