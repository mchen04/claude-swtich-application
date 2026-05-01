use std::io::Write;
use std::time::Duration;

use crossterm::{cursor, terminal, ExecutableCommand};
use serde::Serialize;

use crate::cli::{GlobalOpts, UsageArgs};
use crate::error::Result;
use crate::output::{emit_json, emit_text, OutputOpts};
use crate::paths::Paths;
use crate::usage::{ccusage::CcusageClient, ActiveBlock, DailyTotal};

#[derive(Debug, Serialize)]
struct UsageReport {
    mode: &'static str,
    blocks: Vec<ActiveBlock>,
    daily: Vec<DailyTotal>,
}

pub fn run(_paths: &Paths, global: &GlobalOpts, args: &UsageArgs) -> Result<()> {
    let client = CcusageClient::new();
    if args.watch {
        run_watch(&client, global, args)?;
        return Ok(());
    }
    let report = build(&client, args)?;
    if global.json {
        emit_json(&report)?;
    } else {
        emit_text(
            OutputOpts {
                json: false,
                no_color: global.no_color,
            },
            &TextReport(&report),
        )?;
    }
    Ok(())
}

fn build(client: &CcusageClient, args: &UsageArgs) -> Result<UsageReport> {
    let blocks = if !args.daily && !args.monthly {
        client.active_blocks().unwrap_or_default()
    } else {
        Vec::new()
    };
    let daily = if args.daily || args.monthly {
        client.daily().unwrap_or_default()
    } else {
        Vec::new()
    };
    let mode = if args.monthly {
        "monthly"
    } else if args.daily {
        "daily"
    } else {
        "blocks"
    };
    Ok(UsageReport {
        mode,
        blocks,
        daily,
    })
}

fn run_watch(client: &CcusageClient, global: &GlobalOpts, args: &UsageArgs) -> Result<()> {
    let mut stdout = std::io::stdout();
    let _ = stdout.execute(cursor::Hide);
    let mut first = true;
    loop {
        let report = build(client, args)?;
        if first {
            first = false;
        } else {
            // Move up to the start of the previous render and clear from cursor down.
            let _ = stdout.execute(cursor::MoveToColumn(0));
            // Repaint requires we know how many lines we wrote. Use ClearAll for
            // simplicity — visually cleaner with a small frame.
            let _ = stdout.execute(terminal::Clear(terminal::ClearType::FromCursorDown));
            let _ = stdout.execute(cursor::MoveToPreviousLine(20));
            let _ = stdout.execute(terminal::Clear(terminal::ClearType::FromCursorDown));
        }
        if global.json {
            serde_json::to_writer_pretty(&mut stdout, &report)?;
            writeln!(stdout, "\n(updated {})", chrono::Utc::now().to_rfc3339())?;
        } else {
            write!(stdout, "{}", TextReport(&report))?;
            writeln!(stdout, "(updated {})", chrono::Utc::now().to_rfc3339())?;
        }
        stdout.flush().ok();
        std::thread::sleep(Duration::from_millis(1000));
    }
}

struct TextReport<'a>(&'a UsageReport);

impl std::fmt::Display for TextReport<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0.mode {
            "blocks" => {
                if self.0.blocks.is_empty() {
                    writeln!(f, "no active 5-hour block (run a `claude` command first)")?;
                }
                for b in &self.0.blocks {
                    let total =
                        b.tokens_in + b.tokens_out + b.cache_creation_tokens + b.cache_read_tokens;
                    writeln!(
                        f,
                        "block: {} tokens in/out={}/{} cache r/w={}/{} ${:.2}",
                        total,
                        b.tokens_in,
                        b.tokens_out,
                        b.cache_read_tokens,
                        b.cache_creation_tokens,
                        b.cost_usd,
                    )?;
                    if let (Some(burn), Some(reset)) = (b.burn_rate_per_min, b.resets_at.as_deref())
                    {
                        writeln!(f, "       burn={:.1}/min  resets={}", burn, reset)?;
                    }
                }
            }
            "daily" | "monthly" => {
                for d in &self.0.daily {
                    writeln!(
                        f,
                        "{}: {} tokens ${:.2}  models {}",
                        d.date,
                        d.total_tokens,
                        d.cost_usd,
                        d.models.join(",")
                    )?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
