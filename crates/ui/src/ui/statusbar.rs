//! The bar at the bottom. From the left: what the last run gave, and the
//! SQL as it was really sent to the database, with anything dbpadu added
//! marked. From the right: the engine, the log switch, then the transaction.

use eframe::egui::{self, RichText};

use dbpadu_engine::protocol::QueryResult;

use crate::{
    app::{App, EngineState, Link, Outcome},
    theme::Theme,
};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let theme = app.theme;
    let mut restart = false;
    let mut toggle_log = false;
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        // Far right: the engine.
        let (color, text, hover) = match &app.engine_state {
            EngineState::Starting => (
                theme.warn,
                "Engine starting…".to_string(),
                "The query engine is starting.".to_string(),
            ),
            EngineState::Ready(hello) => (
                theme.ok,
                "Engine ready".to_string(),
                format!(
                    "Query engine {} on Java {}.\nIt never returns more than {} rows for one query.",
                    hello.version, hello.java, hello.max_rows
                ),
            ),
            EngineState::Failed(reason) => (
                theme.danger,
                "Engine stopped · click to start".to_string(),
                reason.clone(),
            ),
        };
        let failed = matches!(app.engine_state, EngineState::Failed(_));
        let label = ui
            .add(
                egui::Label::new(RichText::new(text).small().color(if failed {
                    theme.danger
                } else {
                    theme.text_dim
                }))
                .sense(egui::Sense::click()),
            )
            .on_hover_text(hover);
        if failed && label.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            restart = true;
        }
        super::dot(ui, color);

        // Next to it: opens and closes the log.
        ui.separator();
        let open = app.data.settings.log_open;
        let log = ui
            .add(
                egui::Label::new(RichText::new("Log").small().color(if open {
                    theme.accent_text
                } else {
                    theme.text_dim
                }))
                .sense(egui::Sense::click()),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(if open {
                "Hide the SQL and engine log"
            } else {
                "Show the SQL and engine log"
            });
        if log.clicked() {
            toggle_log = true;
        }

        // Next to it: the transaction of the connection that is shown.
        let tab = app.current_tab();
        let link = tab.and_then(|tab| app.links.get(&tab.connection));
        if let Some(link) = link.filter(|link| link.connected()) {
            ui.separator();
            transaction(ui, &theme, link);
        }

        ui.separator();
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            let Some(tab) = tab else {
                ui.label(
                    RichText::new(super::count(
                        app.data.connections.len(),
                        "connection",
                        "connections",
                    ))
                    .small()
                    .color(theme.text_dim),
                );
                return;
            };
            if let Some(running) = &tab.running {
                ui.spinner();
                ui.label(
                    RichText::new(format!(
                        "Running… {:.1} s",
                        running.since.elapsed().as_secs_f32()
                    ))
                    .small()
                    .color(theme.text_dim),
                );
                return;
            }
            match &tab.outcome {
                None => {
                    ui.label(RichText::new("Ready").small().color(theme.text_dim));
                }
                Some(Outcome::Error(_)) => {
                    ui.label(RichText::new("Failed").small().color(theme.danger));
                }
                Some(Outcome::Result(result)) => {
                    ui.label(RichText::new(summary(result)).small().color(theme.text_dim));
                    ui.separator();
                    executed(ui, &theme, result);
                }
            }
        });
    });
    if restart {
        app.start_engine();
    }
    if toggle_log {
        app.set_log_open(!app.data.settings.log_open);
    }
}

fn transaction(ui: &mut egui::Ui, theme: &Theme, link: &Link) {
    let (text, color, hover) = if link.tx.auto_commit {
        (
            "Auto-commit".to_string(),
            theme.text_dim,
            "Every change is saved to the database as soon as it runs.",
        )
    } else if link.tx.pending == 0 {
        (
            "Transaction · nothing to commit".to_string(),
            theme.text_dim,
            "Changes wait until you click Commit. Nothing is waiting now.",
        )
    } else {
        (
            format!(
                "Transaction · {} not committed",
                super::count(link.tx.pending as usize, "change", "changes")
            ),
            theme.warn,
            "These changes are not saved yet. Click Commit to keep them or Rollback to undo them.",
        )
    };
    ui.label(RichText::new(text).small().color(color))
        .on_hover_text(hover);
}

/// "500 rows · 12 ms" or "3 rows changed · 12 ms".
pub fn summary(result: &QueryResult) -> String {
    let time = super::duration(result.elapsed_ms);
    if result.has_rows() {
        format!(
            "{} · {time}",
            super::count(result.rows.len(), "row", "rows")
        )
    } else if result.update_count >= 0 {
        format!(
            "{} changed · {time}",
            super::count(result.update_count as usize, "row", "rows")
        )
    } else {
        format!("Done · {time}")
    }
}

/// The SQL as it was sent, on one line. What dbpadu added is in the
/// warning color. Click to copy.
fn executed(ui: &mut egui::Ui, theme: &Theme, result: &QueryResult) {
    let sql = super::one_line(&result.executed_sql);
    let font = egui::FontId::monospace(12.0);
    let mut job = egui::text::LayoutJob::default();
    let plain = egui::text::TextFormat::simple(font.clone(), theme.text_dim);
    let marked = egui::text::TextFormat::simple(font.clone(), theme.warn);
    job.append(
        "Ran: ",
        0.0,
        egui::text::TextFormat::simple(egui::FontId::proportional(12.0), theme.text_dim),
    );
    // How many characters fit, so the added part is never the part cut off.
    let prefix = ui.fonts_mut(|fonts| {
        "Ran: "
            .chars()
            .map(|c| fonts.glyph_width(&egui::FontId::proportional(12.0), c))
            .sum::<f32>()
    });
    let glyph = ui.fonts_mut(|fonts| fonts.glyph_width(&font, 'a')).max(1.0);
    let room = ((ui.available_width() - prefix - 8.0) / glyph)
        .floor()
        .max(0.0) as usize;
    let mut hover = format!("Sent to the database:\n\n{}", result.executed_sql);
    match result
        .added
        .as_deref()
        .and_then(|added| split_added(&sql, added))
    {
        Some((before, added, after)) => {
            let (before, after) = fit(before, added, after, room);
            job.append(&before, 0.0, plain.clone());
            job.append(added, 0.0, marked);
            job.append(&after, 0.0, plain);
            hover.push_str(&format!(
                "\n\ndbpadu added {added} so the database cannot send back an endless result."
            ));
        }
        None => {
            let (sql, _) = fit(&sql, "", "", room);
            job.append(&sql, 0.0, plain);
            if result.has_rows() {
                hover.push_str(&format!(
                    "\n\nNothing was added to the SQL. The engine still stops at {} rows.",
                    result.limit
                ));
            }
        }
    }
    hover.push_str("\n\nClick to copy.");
    let label = ui
        .add(egui::Label::new(job).truncate().sense(egui::Sense::click()))
        .on_hover_text(hover);
    if label.clicked() {
        ui.ctx().copy_text(result.executed_sql.clone());
    }
}

/// `sql` cut in three around what was added. An added limit is at the end;
/// SQL Server's `TOP (n)` is near the start.
fn split_added<'a>(sql: &'a str, added: &str) -> Option<(&'a str, &'a str, &'a str)> {
    let at = if sql.ends_with(added) {
        sql.len() - added.len()
    } else {
        sql.find(added)?
    };
    Some((
        &sql[..at],
        &sql[at..at + added.len()],
        &sql[at + added.len()..],
    ))
}

/// Shorten the SQL around what was added so that all of it fits in `room`
/// characters, with "…" where text was left out. What was added is never
/// shortened: text before it loses its end, and if that is not enough, the
/// text after it loses its end too.
fn fit(before: &str, added: &str, after: &str, room: usize) -> (String, String) {
    let length = |text: &str| text.chars().count();
    let total = length(before) + length(added) + length(after);
    if total <= room {
        return (before.to_string(), after.to_string());
    }
    let cut = |text: &str, keep: usize| -> String {
        if length(text) <= keep {
            text.to_string()
        } else {
            let kept: String = text.chars().take(keep.saturating_sub(2)).collect();
            format!("{kept}… ")
        }
    };
    let for_before = room.saturating_sub(length(added) + length(after));
    if for_before >= 12 || after.is_empty() {
        return (cut(before, for_before), after.to_string());
    }
    // A long tail, as with SQL Server's TOP near the start.
    let for_after = room.saturating_sub(length(added) + length(before));
    (
        before.to_string(),
        cut(after, for_after).trim_end().to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_was_added_is_found() {
        assert_eq!(
            split_added("select 'LIMIT 5' from t LIMIT 5", "LIMIT 5"),
            Some(("select 'LIMIT 5' from t ", "LIMIT 5", ""))
        );
        assert_eq!(
            split_added("select TOP (5) * from t", "TOP (5)"),
            Some(("select ", "TOP (5)", " * from t"))
        );
        assert_eq!(split_added("select 1", "LIMIT 5"), None);
    }

    #[test]
    fn long_sql_is_shortened_but_never_what_was_added() {
        let before = "select a, b, c, d, e, f, g from a_long_table_name ";
        let (cut, after) = fit(before, "LIMIT 500", "", 30);
        assert_eq!(after, "");
        assert!(cut.ends_with("… "), "{cut}");
        assert_eq!(cut.chars().count() + "LIMIT 500".len(), 30);

        let (start, tail) = fit("select ", "TOP (500)", " a, b, c, d, e, f, g from t", 30);
        assert_eq!(start, "select ");
        assert!(tail.ends_with('…'), "{tail}");

        let whole = fit("select 1 ", "LIMIT 5", "", 80);
        assert_eq!(whole, ("select 1 ".to_string(), String::new()));
    }

    #[test]
    fn summaries() {
        let rows = QueryResult {
            kind: "rows".into(),
            rows: vec![vec![None], vec![None]],
            elapsed_ms: 12,
            ..Default::default()
        };
        assert_eq!(summary(&rows), "2 rows · 12 ms");
        let update = QueryResult {
            kind: "update".into(),
            update_count: 1,
            elapsed_ms: 2_500,
            ..Default::default()
        };
        assert_eq!(summary(&update), "1 row changed · 2.5 s");
    }
}
