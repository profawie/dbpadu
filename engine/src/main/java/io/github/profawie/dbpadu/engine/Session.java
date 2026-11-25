package io.github.profawie.dbpadu.engine;

import com.google.gson.JsonArray;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import java.sql.Clob;
import java.sql.Connection;
import java.sql.DatabaseMetaData;
import java.sql.ResultSet;
import java.sql.ResultSetMetaData;
import java.sql.SQLException;
import java.sql.Statement;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;
import java.util.Set;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * One open database connection. Everything except {@link #cancel()} runs on
 * the session's own thread, one request after the other, in the order the
 * app sent them.
 */
final class Session {
    /** No request can ask for more rows than this, whatever it says. */
    static final int MAX_ROWS = 10_000;
    static final int DEFAULT_ROWS = 500;
    /** Longer values are cut, so one huge cell cannot fill the memory. */
    static final int MAX_CELL_CHARS = 10_000;

    final String kind;
    final ExecutorService worker;
    private final Connection connection;
    /** Statements run since the last commit or rollback, with auto-commit off. */
    private int pending;
    private volatile Statement running;

    Session(String id, String kind, Connection connection) {
        this.kind = kind;
        this.connection = connection;
        this.worker = Executors.newSingleThreadExecutor(task -> {
            Thread thread = new Thread(task, "session-" + id);
            thread.setDaemon(true);
            return thread;
        });
    }

    static int clampLimit(int asked) {
        if (asked <= 0) {
            return DEFAULT_ROWS;
        }
        return Math.min(asked, MAX_ROWS);
    }

    JsonObject info() throws SQLException {
        DatabaseMetaData meta = connection.getMetaData();
        JsonObject info = new JsonObject();
        info.addProperty("product", meta.getDatabaseProductName());
        info.addProperty("version", meta.getDatabaseProductVersion());
        info.add("tx", tx());
        return info;
    }

    JsonObject tx() throws SQLException {
        JsonObject tx = new JsonObject();
        tx.addProperty("autoCommit", connection.getAutoCommit());
        tx.addProperty("pending", pending);
        return tx;
    }

    JsonObject execute(String sql, int askedLimit) throws SQLException {
        int limit = clampLimit(askedLimit);
        LimitGuard.Guarded guarded = LimitGuard.apply(sql, kind, limit);
        long started = System.nanoTime();
        JsonObject result = new JsonObject();
        result.addProperty("executedSql", guarded.sql());
        if (guarded.changed()) {
            result.addProperty("added", guarded.added());
        }
        result.addProperty("limit", limit);
        try (Statement statement = connection.createStatement()) {
            // The second guard: the driver stops at `limit` rows even when
            // the SQL has no limit or a larger one.
            statement.setMaxRows(limit);
            statement.setFetchSize(Math.min(limit, 500));
            running = statement;
            boolean hasRows = statement.execute(guarded.sql());
            if (hasRows) {
                try (ResultSet rows = statement.getResultSet()) {
                    readRows(rows, limit, result);
                }
            } else {
                result.addProperty("kind", "update");
                result.addProperty("updateCount", statement.getUpdateCount());
                if (!connection.getAutoCommit()) {
                    pending++;
                }
            }
        } finally {
            running = null;
        }
        result.addProperty("elapsedMs", (System.nanoTime() - started) / 1_000_000);
        result.add("tx", tx());
        return result;
    }

    private static void readRows(ResultSet rows, int limit, JsonObject result) throws SQLException {
        ResultSetMetaData meta = rows.getMetaData();
        int count = meta.getColumnCount();
        JsonArray columns = new JsonArray();
        for (int i = 1; i <= count; i++) {
            JsonObject column = new JsonObject();
            column.addProperty("name", meta.getColumnLabel(i));
            column.addProperty("type", meta.getColumnTypeName(i));
            columns.add(column);
        }
        JsonArray data = new JsonArray();
        int read = 0;
        while (read < limit && rows.next()) {
            JsonArray row = new JsonArray();
            for (int i = 1; i <= count; i++) {
                String text = cell(rows, i);
                if (text == null) {
                    row.add(JsonNull.INSTANCE);
                } else {
                    row.add(text);
                }
            }
            data.add(row);
            read++;
        }
        result.addProperty("kind", "rows");
        result.add("columns", columns);
        result.add("rows", data);
        // The database may have more rows than were asked for.
        result.addProperty("limitReached", read >= limit);
    }

    private static String cell(ResultSet rows, int index) throws SQLException {
        Object value;
        try {
            value = rows.getObject(index);
        } catch (SQLException | RuntimeException unreadable) {
            value = rows.getString(index);
        }
        if (value == null) {
            return null;
        }
        if (value instanceof byte[] bytes) {
            return "[binary, " + bytes.length + " bytes]";
        }
        String text;
        if (value instanceof Clob clob) {
            long length = clob.length();
            text = clob.getSubString(1, (int) Math.min(length, MAX_CELL_CHARS + 1));
        } else {
            text = String.valueOf(value);
        }
        if (text.length() > MAX_CELL_CHARS) {
            return text.substring(0, MAX_CELL_CHARS) + "… [cut, " + text.length() + " characters]";
        }
        return text;
    }

    /** Stops the statement that is running now, if any. Called from another thread. */
    void cancel() throws SQLException {
        Statement statement = running;
        if (statement != null) {
            statement.cancel();
        }
    }

    JsonObject setAutoCommit(boolean value) throws SQLException {
        // Turning auto-commit on commits what is pending (JDBC rule).
        connection.setAutoCommit(value);
        pending = 0;
        return tx();
    }

    JsonObject commit() throws SQLException {
        connection.commit();
        pending = 0;
        return tx();
    }

    JsonObject rollback() throws SQLException {
        connection.rollback();
        pending = 0;
        return tx();
    }

    /**
     * The first level of the tree. Most databases have schemas; MySQL and
     * MariaDB call them catalogs; SQLite has neither.
     */
    JsonObject schemas() throws SQLException {
        DatabaseMetaData meta = connection.getMetaData();
        List<String> names = new ArrayList<>();
        String mode = "schema";
        try (ResultSet rows = meta.getSchemas()) {
            while (rows.next()) {
                names.add(rows.getString("TABLE_SCHEM"));
            }
        }
        String current = null;
        if (names.isEmpty()) {
            mode = "catalog";
            try (ResultSet rows = meta.getCatalogs()) {
                while (rows.next()) {
                    names.add(rows.getString("TABLE_CAT"));
                }
            }
            if (names.isEmpty()) {
                mode = "none";
            } else {
                current = connection.getCatalog();
            }
        } else {
            try {
                current = connection.getSchema();
            } catch (SQLException | AbstractMethodError notSupported) {
                current = null;
            }
        }
        JsonObject result = new JsonObject();
        result.addProperty("mode", mode);
        JsonArray items = new JsonArray();
        names.forEach(items::add);
        result.add("items", items);
        if (current != null) {
            result.addProperty("current", current);
        }
        return result;
    }

    JsonObject tables(String mode, String schema) throws SQLException {
        DatabaseMetaData meta = connection.getMetaData();
        String quote = quote(meta);
        String catalog = "catalog".equals(mode) ? schema : null;
        String schemaPattern = "schema".equals(mode) ? schema : null;
        JsonArray items = new JsonArray();
        try (ResultSet rows = meta.getTables(catalog, schemaPattern, "%", null)) {
            while (rows.next()) {
                String type = rows.getString("TABLE_TYPE");
                String upper = type == null ? "TABLE" : type.toUpperCase(Locale.ROOT);
                boolean view = upper.contains("VIEW");
                if (!(view || upper.contains("TABLE")) || upper.startsWith("SYSTEM")) {
                    continue;
                }
                String name = rows.getString("TABLE_NAME");
                JsonObject table = new JsonObject();
                table.addProperty("name", name);
                table.addProperty("view", view);
                String quoted = quote + name.replace(quote, quote + quote) + quote;
                if (!"none".equals(mode) && schema != null) {
                    quoted = quote + schema.replace(quote, quote + quote) + quote + "." + quoted;
                }
                // Ready to be put after FROM.
                table.addProperty("sqlName", quoted);
                items.add(table);
            }
        }
        JsonObject result = new JsonObject();
        result.add("items", items);
        return result;
    }

    JsonObject columns(String mode, String schema, String table) throws SQLException {
        DatabaseMetaData meta = connection.getMetaData();
        String catalog = "catalog".equals(mode) ? schema : null;
        String schemaName = "schema".equals(mode) ? schema : null;
        Set<String> keys = new HashSet<>();
        try (ResultSet rows = meta.getPrimaryKeys(catalog, schemaName, table)) {
            while (rows.next()) {
                keys.add(rows.getString("COLUMN_NAME"));
            }
        } catch (SQLException noKeys) {
            // Views and some drivers have none to report.
        }
        JsonArray items = new JsonArray();
        try (ResultSet rows = meta.getColumns(catalog, schemaName, table, "%")) {
            while (rows.next()) {
                String name = rows.getString("COLUMN_NAME");
                JsonObject column = new JsonObject();
                column.addProperty("name", name);
                column.addProperty("type", rows.getString("TYPE_NAME"));
                column.addProperty("nullable", rows.getInt("NULLABLE") != DatabaseMetaData.columnNoNulls);
                column.addProperty("key", keys.contains(name));
                items.add(column);
            }
        }
        JsonObject result = new JsonObject();
        result.add("items", items);
        return result;
    }

    private static String quote(DatabaseMetaData meta) throws SQLException {
        String quote = meta.getIdentifierQuoteString();
        return quote == null || quote.isBlank() ? "\"" : quote.trim();
    }

    void close() {
        worker.shutdownNow();
        try {
            Statement statement = running;
            if (statement != null) {
                statement.cancel();
            }
        } catch (SQLException ignored) {
            // Closing anyway.
        }
        try {
            if (!connection.getAutoCommit()) {
                // Nothing is committed behind the user's back.
                connection.rollback();
            }
        } catch (SQLException ignored) {
            // Closing anyway.
        }
        try {
            connection.close();
        } catch (SQLException ignored) {
            // Already gone.
        }
    }
}
