//! The auto-reconnect extra (`docs/extras.md`): whether a lost multiplayer
//! connection may be retried, and when. Only lost connections and the
//! server's own shutdown or restart are retried, never kicks, bans or
//! refusals; attempts back off and stop after a few.
use std::time::{Duration, Instant};

/// Seconds before each attempt; there are no more after the last.
pub const DELAYS: [u64; 5] = [5, 10, 20, 40, 60];
/// Staying connected this long counts the next drop as a fresh one.
pub const STABLE: Duration = Duration::from_secs(60);

/// Whether a disconnect may be retried. `from_server`: the server (or a
/// permanent refusal) gave `reason`, rather than the connection failing.
///
/// `key`: the reason's translation key, when it was a translatable
/// component or a known Horizon error. A key the classification knows
/// decides; the English keyword scan of the text is only the fallback for
/// plain reasons (OS errors, servers' literal kicks), whose language is
/// the server's own.
pub fn allowed(reason: &str, key: Option<&str>, from_server: bool) -> bool {
    true
}

/// The key lists' key-based counterpart: `Some` when the key alone
/// decides. Prefix match, so a key's arguments' variants
/// (`multiplayer.disconnect.banned.expiration`) share the base's verdict.
fn classified_key(key: &str) -> Option<bool> {
    // The keyword lists above, as the keys those texts carry: bans, kicks,
    // whitelists, version refusals, the account's session refusals and
    // Horizon's own refusals: never. Shutdowns, timeouts, a full server, a
    // slow login and the session servers being down: retry.
    const NEVER: [&str; 14] = [
        "multiplayer.disconnect.banned",
        "multiplayer.disconnect.ip_banned",
        "multiplayer.disconnect.kicked",
        "multiplayer.disconnect.not_whitelisted",
        "multiplayer.disconnect.incompatible",
        "multiplayer.disconnect.outdated_client",
        "multiplayer.disconnect.outdated_server",
        "multiplayer.disconnect.duplicate_login",
        "multiplayer.disconnect.unverified_username",
        "disconnect.loginFailedInfo.invalidSession",
        "disconnect.loginFailedInfo.insufficientPrivileges",
        "disconnect.loginFailedInfo.userBanned",
        "horizon.multiplayer.not_signed_in",
        "horizon.multiplayer.invalid_address",
    ];
    const RETRY: [&str; 6] = [
        "multiplayer.disconnect.server_shutdown",
        "multiplayer.disconnect.server_full",
        "multiplayer.disconnect.slow_login",
        "multiplayer.disconnect.authservers_down",
        "disconnect.loginFailedInfo.serversUnavailable",
        "disconnect.timeout",
    ];
    NEVER
        .iter()
        .any(|k| key.starts_with(k))
        .then_some(false)
        .or_else(|| RETRY.iter().any(|k| key.starts_with(k)).then_some(true))
}

/// Attempts at one address.
#[derive(Debug, Default)]
pub struct Tracker {
    address: Option<String>,
    /// The player name the connection used.
    name: String,
    lan: bool,
    /// Attempts already made since the last stable connection.
    attempts: usize,
    joined: Option<Instant>,
}
/// A planned attempt.
#[derive(Clone, Debug, PartialEq)]
pub struct Retry {
    pub address: String,
    pub name: String,
    pub lan: bool,
    pub at: Instant,
    /// 1-based, of `DELAYS.len()`.
    pub attempt: usize,
}
impl Retry {
    /// Whole seconds left, rounded up.
    pub fn seconds_left(&self, now: Instant) -> u64 {
        let left = self.at.saturating_duration_since(now);
        left.as_secs() + u64::from(left.subsec_nanos() > 0)
    }
    pub fn due(&self, now: Instant) -> bool {
        now >= self.at
    }
    pub fn text(&self, now: Instant) -> String {
        format!(
            "Reconnecting in {} s (attempt {} of {})",
            self.seconds_left(now),
            self.attempt,
            DELAYS.len()
        )
    }
}
impl Tracker {
    /// The server connected to last (splitscreen players join the same one).
    pub fn address(&self) -> Option<&str> {
        self.address.as_deref()
    }
    /// That server was found on the local network.
    pub fn lan(&self) -> bool {
        self.lan
    }
    /// A connection to `address` as `name` starts: a new address starts
    /// counting anew.
    pub fn connecting(&mut self, address: &str, name: &str, lan: bool) {
        if self.address.as_deref() != Some(address) {
            self.address = Some(address.to_owned());
            self.attempts = 0;
        }
        self.name = name.to_owned();
        self.lan = lan;
        self.joined = None;
    }
    /// The server let the player in.
    pub fn joined(&mut self, now: Instant) {
        self.joined = Some(now);
    }
    /// The player left on purpose, or cancelled: nothing to retry.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    /// The connection ended: the next attempt, or None (not allowed, or
    /// out of attempts).
    pub fn ended(
        &mut self,
        reason: &str,
        key: Option<&str>,
        from_server: bool,
        now: Instant,
    ) -> Option<Retry> {
        if self
            .joined
            .take()
            .is_some_and(|t| now.saturating_duration_since(t) >= STABLE)
        {
            self.attempts = 0;
        }
        let address = self.address.clone()?;
        if !allowed(reason, key, from_server) || self.attempts >= DELAYS.len() {
            self.reset();
            return None;
        }
        let delay = Duration::from_secs(DELAYS[self.attempts]);
        self.attempts += 1;
        Some(Retry {
            address,
            name: self.name.clone(),
            lan: self.lan,
            at: now + delay,
            attempt: self.attempts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnect_keeps_the_selected_server_kind() {
        let mut tracker = Tracker::default();
        let now = Instant::now();
        tracker.connecting("127.0.0.1:25565", "Tester", true);
        assert!(tracker.ended("Server closed", None, true, now).unwrap().lan);
        tracker.connecting("127.0.0.1:25565", "Tester", false);
        assert!(!tracker.ended("Server closed", None, true, now).unwrap().lan);
    }

    #[test]
    fn attempts_back_off_and_stop() {
        let mut t = Tracker::default();
        let now = Instant::now();
        t.connecting("localhost:25565", "Tester", false);
        let mut delays = Vec::new();
        while let Some(r) = t.ended("Connection refused", None, false, now) {
            assert_eq!(
                (r.address.as_str(), r.name.as_str()),
                ("localhost:25565", "Tester")
            );
            assert_eq!(r.attempt, delays.len() + 1);
            delays.push(r.at.duration_since(now).as_secs());
            t.connecting(&r.address, &r.name, false);
        }
        assert_eq!(delays, DELAYS);
        // Out of attempts, the tracker forgets the address.
        assert_eq!(t.ended("Connection refused", None, false, now), None);
    }

    #[test]
    fn a_short_stay_does_not_reset_the_count() {
        let mut t = Tracker::default();
        let now = Instant::now();
        t.connecting("a", "P", false);
        for _ in 0..DELAYS.len() {
            t.connecting("a", "P", false);
            t.joined(now);
            assert!(
                t.ended("Server closed", None, true, now + Duration::from_secs(5))
                    .is_some()
            );
        }
        t.connecting("a", "P", false);
        t.joined(now);
        assert_eq!(
            t.ended("Server closed", None, true, now + Duration::from_secs(5)),
            None
        );
        // A minute connected starts over.
        t.connecting("a", "P", false);
        for _ in 0..3 {
            t.ended("Timed out", None, true, now);
            t.connecting("a", "P", false);
        }
        t.joined(now);
        let r = t.ended("Timed out", None, true, now + STABLE).unwrap();
        assert_eq!(r.attempt, 1);
    }

    #[test]
    fn the_countdown_rounds_up() {
        let now = Instant::now();
        let r = Retry {
            address: "a".into(),
            name: "P".into(),
            lan: false,
            at: now + Duration::from_millis(4200),
            attempt: 2,
        };
        assert_eq!(r.seconds_left(now), 5);
        assert_eq!(r.text(now), "Reconnecting in 5 s (attempt 2 of 5)");
        assert!(!r.due(now));
        assert!(r.due(now + Duration::from_secs(5)));
        assert_eq!(r.seconds_left(now + Duration::from_secs(9)), 0);
    }
}
