use std::fmt;
use std::time::SystemTime;

use serde::Serialize;

use crate::commands::list;
use crate::error::Result;
use crate::keychain::Keychain;
use crate::paths::Paths;
use crate::profile::{human_expiry, ProfileSummary};
use crate::state::State;
use crate::usage::{
    ccusage::CcusageClient, session_tags, stats_cache, ActiveBlock, DailyByProfile, DailyTotal,
};

#[derive(Debug, Serialize)]
pub struct DashboardSnapshot {
    pub active: Option<ProfileSummary>,
    pub profiles: Vec<ProfileSummary>,
    pub active_block: Option<ActiveBlock>,
    pub today_total: Option<DailyTotal>,
    pub today_by_profile: Vec<DailyByProfile>,
    pub generated_at: String,
    pub warnings: Vec<String>,
}

pub fn snapshot(
    paths: &Paths,
    kc: &dyn Keychain,
    ccusage: Option<&CcusageClient>,
) -> Result<DashboardSnapshot> {
    let state = State::load(&paths.state_file()).unwrap_or_default();
    let list_report = list::build(paths, kc)?;
    let active = list_report.profiles.iter().find(|p| p.is_active).cloned();

    let mut warnings = Vec::new();

    let (active_block, today_total) = match ccusage {
        Some(client) => {
            let blocks = client.active_blocks().unwrap_or_else(|e| {
                warnings.push(format!("ccusage blocks: {e}"));
                Vec::new()
            });
            let daily = client.daily().unwrap_or_else(|e| {
                warnings.push(format!("ccusage daily: {e}"));
                Vec::new()
            });
            (
                blocks.into_iter().next(),
                today_from(daily, today_iso().as_str()),
            )
        }
        None => (None, None),
    };

    // Stats cache + session tags → today_by_profile.
    let stats = stats_cache::load(&paths.stats_cache()).unwrap_or_default();
    let today_iso = today_iso();
    let stats_today = stats.daily_activity.iter().find(|d| d.date == today_iso);

    let tags = session_tags::load(&paths.session_tags_file()).unwrap_or_default();
    let mut today_by_profile = Vec::new();
    if let Some(activity) = stats_today {
        // Without a per-session breakdown in stats-cache, we can only attribute the
        // active profile (best-effort). Sessions tagged today are included; the rest
        // bucket as "unknown" if `tags` is empty.
        let active_name = state.active.clone().unwrap_or_else(|| "unknown".into());
        let tagged_today: u64 = tags
            .iter()
            .filter(|t| t.tagged_at_ms / 86_400_000 == today_unix_day())
            .count() as u64;
        if tagged_today > 0 {
            today_by_profile.push(DailyByProfile {
                profile: active_name.clone(),
                date: activity.date.clone(),
                messages: activity.message_count,
                sessions: tagged_today.min(activity.session_count),
                tool_calls: activity.tool_call_count,
            });
            let unknown = activity.session_count.saturating_sub(tagged_today);
            if unknown > 0 {
                warnings.push(format!(
                    "{unknown} session(s) today started outside the cs wrapper — bucketed as `unknown`"
                ));
                today_by_profile.push(DailyByProfile {
                    profile: "unknown".into(),
                    date: activity.date.clone(),
                    messages: 0,
                    sessions: unknown,
                    tool_calls: 0,
                });
            }
        } else {
            today_by_profile.push(DailyByProfile {
                profile: active_name,
                date: activity.date.clone(),
                messages: activity.message_count,
                sessions: activity.session_count,
                tool_calls: activity.tool_call_count,
            });
        }
    }

    Ok(DashboardSnapshot {
        active,
        profiles: list_report.profiles,
        active_block,
        today_total,
        today_by_profile,
        generated_at: chrono::Utc::now().to_rfc3339(),
        warnings,
    })
}

fn today_iso() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

fn today_unix_day() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64 / 86_400_000)
        .unwrap_or(0)
}

fn today_from(daily: Vec<DailyTotal>, date: &str) -> Option<DailyTotal> {
    daily.into_iter().find(|d| d.date == date)
}

impl fmt::Display for DashboardSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "cs dashboard — {}", self.generated_at)?;
        match &self.active {
            None => writeln!(f, "(no active profile — `cs <name>` to switch)")?,
            Some(p) => {
                let email = p.email.as_deref().unwrap_or("—");
                let plan = p.plan.as_deref().unwrap_or("—");
                let exp = p
                    .expires_in_secs
                    .map(human_expiry)
                    .unwrap_or_else(|| "—".into());
                writeln!(
                    f,
                    "active : {} <{}>  plan={}  token {}",
                    p.name, email, plan, exp
                )?;
            }
        }

        if let Some(block) = &self.active_block {
            let total = block.tokens_in
                + block.tokens_out
                + block.cache_creation_tokens
                + block.cache_read_tokens;
            writeln!(
                f,
                "5h block: {} tokens (in={} out={} cache_r={} cache_w={})  ${:.2}",
                total,
                block.tokens_in,
                block.tokens_out,
                block.cache_read_tokens,
                block.cache_creation_tokens,
                block.cost_usd
            )?;
            if let (Some(burn), Some(reset)) = (block.burn_rate_per_min, block.resets_at.as_deref())
            {
                writeln!(f, "         burn ~{:.1}/min  resets {}", burn, reset)?;
            }
        }

        if let Some(today) = &self.today_total {
            writeln!(
                f,
                "today  : {} tokens  ${:.2}  models {}",
                today.total_tokens,
                today.cost_usd,
                today.models.join(",")
            )?;
        }

        if !self.today_by_profile.is_empty() {
            writeln!(f, "by profile (today):")?;
            for row in &self.today_by_profile {
                writeln!(
                    f,
                    "  {:<12} sessions={} messages={} tools={}",
                    row.profile, row.sessions, row.messages, row.tool_calls
                )?;
            }
        }

        if !self.warnings.is_empty() {
            writeln!(f)?;
            for w in &self.warnings {
                writeln!(f, "warn   : {w}")?;
            }
        }

        Ok(())
    }
}
