package io.github.profawie.dbpadu.engine;

import com.google.gson.Gson;
import com.google.gson.JsonArray;
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
import java.sql.Driver;
import java.sql.DriverManager;
import java.sql.SQLException;
import java.util.Map;
import java.util.Properties;
import java.util.ServiceLoader;
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
                case "hello", "test", "connect" -> POOL.execute(() -> run(id, () -> general(op, request)));
                // Must not wait behind the statement it is meant to stop.
                case "cancel" -> run(id, () -> {
                    session(request).cancel();
                    return new JsonObject();
                });
                case "disconnect" -> run(id, () -> {
                    Session session = SESSIONS.remove(text(request, "session"));
                    if (session != null) {
                        session.close();
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
                JsonArray drivers = new JsonArray();
                for (Driver driver : ServiceLoader.load(Driver.class)) {
                    drivers.add(driver.getClass().getName());
                }
                result.add("drivers", drivers);
                return result;
            }
            case "test" -> {
                Session session = new Session("test", text(request, "kind"), open(request));
                try {
                    return session.info();
                } finally {
                    session.close();
                }
            }
            default -> {
                String id = text(request, "session");
                Session session = new Session(id, text(request, "kind"), open(request));
                Session old = SESSIONS.put(id, session);
                if (old != null) {
                    old.close();
                }
                return session.info();
            }
        }
    }

    private static Connection open(JsonObject request) throws SQLException {
        Properties properties = new Properties();
        String user = text(request, "user");
        String password = text(request, "password");
        if (user != null && !user.isEmpty()) {
            properties.setProperty("user", user);
        }
        if (password != null && !password.isEmpty()) {
            properties.setProperty("password", password);
        }
        return DriverManager.getConnection(text(request, "url"), properties);
    }

    private static JsonObject inSession(Session session, String op, JsonObject request) throws SQLException {
        return switch (op) {
            case "execute" -> session.execute(
                text(request, "sql"), request.has("limit") ? request.get("limit").getAsInt() : 0);
            case "autocommit" -> wrapTx(session.setAutoCommit(request.get("value").getAsBoolean()));
            case "commit" -> wrapTx(session.commit());
            case "rollback" -> wrapTx(session.rollback());
            case "schemas" -> session.schemas();
            case "tables" -> session.tables(text(request, "mode"), text(request, "schema"));
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

    private static String text(JsonObject request, String name) {
        JsonElement value = request.get(name);
        return value == null || value.isJsonNull() ? null : value.getAsString();
    }
}
