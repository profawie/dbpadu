package io.github.profawie.dbpadu.engine;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

/**
 * Makes sure a SELECT never asks the database for an unbounded result.
 *
 * <p>When a SELECT has no row limit of its own, one is added to the SQL in
 * the way the database understands ({@code LIMIT n}, {@code TOP (n)} or
 * {@code FETCH FIRST n ROWS ONLY}). The caller is told exactly what was
 * added, so the app can show it.
 *
 * <p>This is the first of two guards. The second one is in {@link Session}:
 * every statement runs with JDBC's {@code setMaxRows}, so even SQL this class
 * leaves alone cannot return more rows than the limit.
 */
final class LimitGuard {
    private LimitGuard() {}

    /** The SQL to run and what was added to it. */
    record Guarded(String sql, String added) {
        /** True when the SQL was changed. */
        boolean changed() {
            return added != null;
        }
    }

    /** A word outside strings, comments and brackets. */
    private record Word(String text, int start, int end) {}

    private record Scan(List<Word> words, int codeEnd, boolean startsWithBracket) {}

    static Guarded apply(String sql, String kind, int limit) {
        Scan scan = scan(sql, "sqlserver".equals(kind));
        List<Word> words = scan.words();
        if (words.isEmpty() || scan.startsWithBracket()) {
            return new Guarded(sql, null);
        }
        String first = words.get(0).text();
        if (!first.equals("SELECT") && !first.equals("WITH")) {
            return new Guarded(sql, null);
        }
        Word select = null;
        for (Word word : words) {
            switch (word.text()) {
                case "SELECT" -> {
                    if (select == null) {
                        select = word;
                    }
                }
                // WITH ... INSERT/UPDATE/DELETE changes data: not a SELECT.
                case "INSERT", "UPDATE", "DELETE", "MERGE" -> {
                    return new Guarded(sql, null);
                }
                // Already limited by the user.
                case "LIMIT", "FETCH", "TOP" -> {
                    return new Guarded(sql, null);
                }
                // SELECT ... INTO, FOR UPDATE, FOR XML: the limit would land
                // in the wrong place. The engine guard still applies.
                case "INTO", "FOR" -> {
                    return new Guarded(sql, null);
                }
                default -> {}
            }
        }
        if (select == null) {
            return new Guarded(sql, null);
        }
        String body = sql.substring(0, scan.codeEnd());
        switch (kind == null ? "" : kind) {
            case "postgres", "mysql", "mariadb", "sqlite" -> {
                String added = "LIMIT " + limit;
                return new Guarded(body + "\n" + added, added);
            }
            case "oracle" -> {
                String added = "FETCH FIRST " + limit + " ROWS ONLY";
                return new Guarded(body + "\n" + added, added);
            }
            case "sqlserver" -> {
                // TOP only limits the SELECT it is written in.
                for (Word word : words) {
                    switch (word.text()) {
                        case "UNION", "EXCEPT", "INTERSECT" -> {
                            return new Guarded(sql, null);
                        }
                        default -> {}
                    }
                }
                int at = select.end();
                int index = words.indexOf(select);
                if (index + 1 < words.size()) {
                    Word next = words.get(index + 1);
                    if (next.text().equals("DISTINCT") || next.text().equals("ALL")) {
                        at = next.end();
                    }
                }
                String added = "TOP (" + limit + ")";
                return new Guarded(body.substring(0, at) + " " + added + body.substring(at), added);
            }
            default -> {
                return new Guarded(sql, null);
            }
        }
    }

    /**
     * Reads the SQL once and collects the words that are not inside a
     * string, a comment or brackets. Also finds where the last piece of real
     * SQL ends, so a limit can be added before a closing {@code ;} or a
     * comment at the end.
     */
    private static Scan scan(String sql, boolean squareBrackets) {
        List<Word> words = new ArrayList<>();
        int depth = 0;
        int codeEnd = 0;
        boolean sawCode = false;
        boolean startsWithBracket = false;
        int length = sql.length();
        int i = 0;
        while (i < length) {
            char c = sql.charAt(i);
            char next = i + 1 < length ? sql.charAt(i + 1) : '\0';
            if (Character.isWhitespace(c)) {
                i++;
            } else if (c == '-' && next == '-') {
                while (i < length && sql.charAt(i) != '\n') {
                    i++;
                }
            } else if (c == '/' && next == '*') {
                int close = sql.indexOf("*/", i + 2);
                i = close < 0 ? length : close + 2;
            } else if (c == '\'' || c == '"' || c == '`') {
                i = skipQuoted(sql, i, c);
                codeEnd = i;
                sawCode = true;
            } else if (c == '[' && squareBrackets) {
                int close = sql.indexOf(']', i + 1);
                i = close < 0 ? length : close + 1;
                codeEnd = i;
                sawCode = true;
            } else if (c == '$' && dollarTagEnd(sql, i) > 0) {
                // PostgreSQL: $$ ... $$ or $tag$ ... $tag$
                int tagEnd = dollarTagEnd(sql, i);
                String tag = sql.substring(i, tagEnd);
                int close = sql.indexOf(tag, tagEnd);
                i = close < 0 ? length : close + tag.length();
                codeEnd = i;
                sawCode = true;
            } else if (c == ';') {
                // Does not move codeEnd: a closing ; is dropped.
                i++;
            } else if (Character.isLetter(c) || c == '_') {
                int start = i;
                while (i < length && (Character.isLetterOrDigit(sql.charAt(i)) || sql.charAt(i) == '_')) {
                    i++;
                }
                if (depth == 0) {
                    words.add(new Word(sql.substring(start, i).toUpperCase(Locale.ROOT), start, i));
                }
                codeEnd = i;
                sawCode = true;
            } else {
                if (c == '(') {
                    if (!sawCode) {
                        startsWithBracket = true;
                    }
                    depth++;
                } else if (c == ')') {
                    depth = Math.max(0, depth - 1);
                }
                i++;
                codeEnd = i;
                sawCode = true;
            }
        }
        return new Scan(words, codeEnd, startsWithBracket);
    }

    /** Index after the closing quote; a doubled quote stays inside. */
    private static int skipQuoted(String sql, int start, char quote) {
        int i = start + 1;
        while (i < sql.length()) {
            if (sql.charAt(i) == quote) {
                if (i + 1 < sql.length() && sql.charAt(i + 1) == quote) {
                    i += 2;
                    continue;
                }
                return i + 1;
            }
            i++;
        }
        return sql.length();
    }

    /** Index after a {@code $tag$} that starts at {@code start}, or -1. */
    private static int dollarTagEnd(String sql, int start) {
        int i = start + 1;
        while (i < sql.length() && (Character.isLetter(sql.charAt(i)) || sql.charAt(i) == '_')) {
            i++;
        }
        return i < sql.length() && sql.charAt(i) == '$' ? i + 1 : -1;
    }
}
