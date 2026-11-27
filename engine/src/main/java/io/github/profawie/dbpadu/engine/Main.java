package io.github.profawie.dbpadu.engine;

import com.google.gson.Gson;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.io.BufferedReader;
import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.sql.Connection;
import java.sql.DriverManager;
import java.sql.SQLException;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Properties;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * The dbpadu engine. The app starts it as a child process and talks to it
 * over standard input and output, one JSON object per line.
 *
 * <pre>
 * request:  {"id": 7, "op": "execute", "session": "...", "sql": "...", "limit": 500}
 * answer:   {"id": 7, "ok": true, "result": {...}}
 *           {"id": 7, "ok": false, "error": "..."}
 * </pre>
 *
 * The engine stops when the app closes its end of standard input.
 */
public final class Main {
    private static final Gson GSON = new Gson();
    private static final Map<String, Session> SESSIONS = new ConcurrentHashMap<>();
    /** For work that has no session yet: hello, test and connect. */
    private static final ExecutorService POOL = Executors.newCachedThreadPool(task -> {
        Thread thread = new Thread(task, "engine");
        thread.setDaemon(true);
        return thread;
    });
    private static PrintStream out;

    private Main() {}

    public static void main(String[] args) throws IOException {
        // Standard output carries the protocol only. Anything a driver
        // prints goes to standard error instead.
        out = new PrintStream(new FileOutputStream(FileDescriptor.out), true, StandardCharsets.UTF_8);
        System.setOut(System.err);
        DriverManager.setLoginTimeout(15);

        BufferedReader in = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
        String line;
        while ((line = in.readLine()) != null) {
            if (line.isBlank()) {
                continue;
            }
            JsonObject request;
            try {
                request = JsonParser.parseString(line).getAsJsonObject();
            } catch (RuntimeException notJson) {
                System.err.println("ignored a line that is not a JSON object");
                continue;
            }
            dispatch(request);
        }
        SESSIONS.values().forEach(Session::close);
    }

    private static void dispatch(JsonObject request) {
        long id = request.has("id") ? request.get("id").getAsLong() : 0;
        String op = text(request, "op");
        try {
            switch (op == null ? "" : op) {
                case "hello", "test", "connect", "driverInfo" ->
                    POOL.execute(() -> run(id, () -> general(op, request)));
                // Must not wait behind the statement it is meant to stop.
                case "cancel" -> run(id, () -> {
                    session(request).cancel();
                    return new JsonObject();
                });
                case "disconnect" -> run(id, () -> {
                    Session session = SESSIONS.remove(text(request, "session"));
                    if (session != null) {
                        session.close();
                        System.err.println("[engine] disconnected " + text(request, "session"));
                    }
                    return new JsonObject();
                });
                default -> {
                    Session session = session(request);
                    session.worker.execute(() -> run(id, () -> inSession(session, op, request)));
                }
            }
        } catch (Exception problem) {
            fail(id, problem);
        }
    }

    private interface Work {
        JsonObject get() throws Exception;
    }

    private static void run(long id, Work work) {
        try {
            JsonObject answer = new JsonObject();
            answer.addProperty("id", id);
            answer.addProperty("ok", true);
            answer.add("result", work.get());
            send(answer);
        } catch (Throwable problem) {
            fail(id, problem);
        }
    }

    private static void fail(long id, Throwable problem) {
        JsonObject answer = new JsonObject();
        answer.addProperty("id", id);
        answer.addProperty("ok", false);
        answer.addProperty("error", message(problem));
        send(answer);
    }

    private static String message(Throwable problem) {
        if (problem instanceof OutOfMemoryError) {
            return "The engine ran out of memory. Use a smaller row limit.";
        }
        if (!(problem instanceof SQLException)) {
            // Unexpected: keep the details for the engine log.
            problem.printStackTrace();
        }
        String message = problem.getMessage();
        if (message == null || message.isBlank()) {
            return problem.getClass().getSimpleName();
        }
        return message.strip();
    }

    private static void send(JsonElement answer) {
        String line = GSON.toJson(answer);
        synchronized (Main.class) {
            out.println(line);
            out.flush();
        }
    }

    private static JsonObject general(String op, JsonObject request) throws SQLException {
        switch (op) {
            case "hello" -> {
                JsonObject result = new JsonObject();
                String version = Main.class.getPackage().getImplementationVersion();
                result.addProperty("version", version == null ? "dev" : version);
                result.addProperty("java", System.getProperty("java.version"));
                result.addProperty("maxRows", Session.MAX_ROWS);
                result.add("drivers", Drivers.bundled());
                return result;
            }
            case "driverInfo" -> {
                return Drivers.describe(libraries(request), text(request, "driverClass"));
            }
            case "test" -> {
                Session session = open("test", request);
                try {
                    return session.info();
                } finally {
                    session.close();
                }
            }
            default -> {
                String id = text(request, "session");
                Session session = open(id, request);
                Session old = SESSIONS.put(id, session);
                if (old != null) {
                    old.close();
                }
                JsonObject info = session.info();
                System.err.println(
                    "[engine] connected " + id + " to " + info.get("product").getAsString()
                        + " " + info.get("version").getAsString());
                return info;
            }
        }
    }

    /** Open the SSH tunnel, if one is asked for, then the connection. */
    private static Session open(String id, JsonObject request) throws SQLException {
        Properties properties = new Properties();
        // The settings of the user's own driver; user and password win.
        if (request.has("properties") && request.get("properties").isJsonObject()) {
            for (Map.Entry<String, JsonElement> property
                    : request.getAsJsonObject("properties").entrySet()) {
                if (property.getValue().isJsonPrimitive()) {
                    properties.setProperty(property.getKey(), property.getValue().getAsString());
                }
            }
        }
        String user = text(request, "user");
        String password = text(request, "password");
        if (user != null && !user.isEmpty()) {
            properties.setProperty("user", user);
        }
        if (password != null && !password.isEmpty()) {
            properties.setProperty("password", password);
        }
        String url = text(request, "url");
        Tunnel tunnel = null;
        if (request.has("ssh") && request.get("ssh").isJsonObject()) {
            tunnel = Tunnel.open(request.getAsJsonObject("ssh"));
            url = url.replace(Tunnel.PORT_PLACEHOLDER, String.valueOf(tunnel.localPort));
        }
        try {
            Connection connection = Drivers.connect(
                url, properties, libraries(request), text(request, "driverClass"));
            return new Session(id, text(request, "kind"), connection, tunnel);
        } catch (SQLException | RuntimeException problem) {
            if (tunnel != null) {
                tunnel.close();
            }
            throw problem;
        }
    }

    private static JsonObject inSession(Session session, String op, JsonObject request) throws SQLException {
        return switch (op) {
            case "execute" -> session.execute(
                text(request, "sql"), request.has("limit") ? request.get("limit").getAsInt() : 0);
            case "export" -> session.export(text(request, "sql"), text(request, "file"));
            case "autocommit" -> wrapTx(session.setAutoCommit(request.get("value").getAsBoolean()));
            case "commit" -> wrapTx(session.commit());
            case "rollback" -> wrapTx(session.rollback());
            case "schemas" -> session.schemas();
            case "tables" -> session.tables(text(request, "mode"), text(request, "schema"));
            case "routines" -> session.routines(text(request, "mode"), text(request, "schema"));
            case "sequences" -> session.sequences(text(request, "schema"));
            case "columns" -> session.columns(
                text(request, "mode"), text(request, "schema"), text(request, "table"));
            default -> throw new SQLException("Unknown request: " + op);
        };
    }

    private static JsonObject wrapTx(JsonObject tx) {
        JsonObject result = new JsonObject();
        result.add("tx", tx);
        return result;
    }

    private static Session session(JsonObject request) throws SQLException {
        Session session = SESSIONS.get(String.valueOf(text(request, "session")));
        if (session == null) {
            throw new SQLException("Not connected.");
        }
        return session;
    }

    /** The jar files of the user's own driver; empty for a driver inside the engine. */
    private static List<String> libraries(JsonObject request) {
        List<String> libraries = new ArrayList<>();
        if (request.has("driverJars") && request.get("driverJars").isJsonArray()) {
            for (JsonElement jar : request.getAsJsonArray("driverJars")) {
                if (jar.isJsonPrimitive() && !jar.getAsString().isBlank()) {
                    libraries.add(jar.getAsString());
                }
            }
        }
        return libraries;
    }

    private static String text(JsonObject request, String name) {
        JsonElement value = request.get(name);
        return value == null || value.isJsonNull() ? null : value.getAsString();
    }
}
