package io.github.profawie.dbpadu.engine;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;

import org.junit.jupiter.api.Test;

class LimitGuardTest {
    private static String run(String sql, String kind) {
        return LimitGuard.apply(sql, kind, 500).sql();
    }

    @Test
    void addsLimitToAPlainSelect() {
        var guarded = LimitGuard.apply("select * from users", "postgres", 500);
        assertEquals("select * from users\nLIMIT 500", guarded.sql());
        assertEquals("LIMIT 500", guarded.added());
    }

    @Test
    void dropsTheClosingSemicolonAndTrailingComment() {
        assertEquals("SELECT 1\nLIMIT 500", run("SELECT 1 ; ", "mysql"));
        assertEquals("SELECT 1\nLIMIT 500", run("SELECT 1; -- all of it", "sqlite"));
        assertEquals("SELECT 1\nLIMIT 500", run("SELECT 1 -- all of it", "sqlite"));
    }

    @Test
    void leavesAQueryThatHasItsOwnLimit() {
        for (String sql : new String[] {
            "select * from t limit 10",
            "select * from t LIMIT 10 OFFSET 5",
            "select * from t order by id fetch first 5 rows only",
        }) {
            var guarded = LimitGuard.apply(sql, "postgres", 500);
            assertEquals(sql, guarded.sql());
            assertNull(guarded.added());
        }
    }

    @Test
    void aLimitInsideASubqueryStringOrCommentDoesNotCount() {
        assertEquals(
            "select * from (select * from t limit 5) x\nLIMIT 500",
            run("select * from (select * from t limit 5) x", "postgres"));
        assertEquals(
            "select 'limit 5' from t\nLIMIT 500", run("select 'limit 5' from t", "postgres"));
        assertEquals(
            "select 'it''s; limit' from t\nLIMIT 500",
            run("select 'it''s; limit' from t", "postgres"));
        assertEquals(
            "select * /* limit 5 */ from t\nLIMIT 500",
            run("select * /* limit 5 */ from t", "postgres"));
        assertEquals(
            "select $$ limit 5 $$ from t\nLIMIT 500",
            run("select $$ limit 5 $$ from t", "postgres"));
    }

    @Test
    void commonTableExpressions() {
        assertEquals(
            "with a as (select 1) select * from a\nLIMIT 500",
            run("with a as (select 1) select * from a", "postgres"));
        String insert = "with a as (select 1) insert into t select * from a";
        assertEquals(insert, run(insert, "postgres"));
    }

    @Test
    void statementsThatAreNotSelectAreLeftAlone() {
        for (String sql : new String[] {
            "update t set a = 1",
            "delete from t",
            "insert into t select * from u",
            "create table t as select * from u",
            "",
            "  -- only a comment",
        }) {
            assertFalse(LimitGuard.apply(sql, "postgres", 500).changed(), sql);
        }
    }

    @Test
    void selectIntoAndForUpdateAreLeftToTheEngineGuard() {
        assertFalse(LimitGuard.apply("select * into u from t", "sqlserver", 500).changed());
        assertFalse(LimitGuard.apply("select * from t for update", "postgres", 500).changed());
    }

    @Test
    void oracleUsesFetchFirst() {
        assertEquals(
            "select * from t\nFETCH FIRST 500 ROWS ONLY", run("select * from t;", "oracle"));
    }

    @Test
    void sqlServerUsesTop() {
        assertEquals("select TOP (500) * from t", run("select * from t", "sqlserver"));
        assertEquals("select distinct TOP (500) a from t", run("select distinct a from t;", "sqlserver"));
        assertEquals(
            "with a as (select 1 x) select TOP (500) * from a",
            run("with a as (select 1 x) select * from a", "sqlserver"));
        assertEquals("select TOP (500) [limit] from t", run("select [limit] from t", "sqlserver"));
        String top = "select top 5 * from t";
        assertEquals(top, run(top, "sqlserver"));
        String union = "select a from t union select a from u";
        assertEquals(union, run(union, "sqlserver"));
    }

    @Test
    void unknownDatabaseIsLeftToTheEngineGuard() {
        assertFalse(LimitGuard.apply("select * from t", "other", 500).changed());
    }

    @Test
    void onlyAPlainSelectIsRunAgainForAnExport() {
        assertEquals("select * from t", LimitGuard.forExport("select * from t;", "postgres"));
        assertEquals(
            "with x as (select 1) select * from x",
            LimitGuard.forExport("with x as (select 1) select * from x -- all", "postgres"));
        assertEquals(
            "(select 1) union (select 2)",
            LimitGuard.forExport("(select 1) union (select 2)", "mysql"));
        // The limit the user wrote stays.
        assertEquals("select * from t limit 5", LimitGuard.forExport("select * from t limit 5", "sqlite"));
        // A word that changes data in a string is only text.
        assertEquals(
            "select 'delete' from t", LimitGuard.forExport("select 'delete' from t", "postgres"));

        assertNull(LimitGuard.forExport("update t set a = 1", "postgres"));
        assertNull(LimitGuard.forExport("delete from t returning *", "postgres"));
        assertNull(LimitGuard.forExport(
            "with gone as (delete from t returning *) select * from gone", "postgres"));
        assertNull(LimitGuard.forExport("select * into copy from t", "sqlserver"));
        assertNull(LimitGuard.forExport("call cleanup()", "mysql"));
        assertNull(LimitGuard.forExport("  -- nothing", "postgres"));
    }
}
