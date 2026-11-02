//! Fast domain matcher and traffic filter for hostlists.

use std::collections::HashSet;

#[derive(Clone, Debug, Default)]
pub struct DomainMatcher {
    domains: HashSet<String>,
}

impl DomainMatcher {
    pub fn new() -> Self {
        Self {
            domains: HashSet::new(),
        }
    }

    pub fn from_domains<I, S>(domains: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut set = HashSet::new();
        for d in domains {
            let trimmed = d.as_ref().trim().to_ascii_lowercase();
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                // Strip leading dot if any
                let norm = trimmed.trim_start_matches('.');
                set.insert(norm.to_string());
            }
        }
        Self { domains: set }
    }

    pub fn add<S: AsRef<str>>(&mut self, domain: S) {
        let trimmed = domain.as_ref().trim().to_ascii_lowercase();
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            let norm = trimmed.trim_start_matches('.');
            self.domains.insert(norm.to_string());
        }
    }

    /// Checks if a domain matches the hostlist.
    /// Supports exact matching as well as hierarchical subdomain matching:
    /// e.g. "discord.com" matches "discord.com" and "gateway.discord.com".
    pub fn matches(&self, domain: &str) -> bool {
        let norm = domain.trim().to_ascii_lowercase();
        let target = norm.trim_end_matches('.');

        if self.domains.contains(target) {
            return true;
        }

        // Check parent domains without any String or Vec allocations
        let mut search_idx = 0;
        while let Some(dot_offset) = target[search_idx..].find('.') {
            let next_dot = search_idx + dot_offset;
            let parent = &target[next_dot + 1..];
            if !parent.is_empty() && self.domains.contains(parent) {
                return true;
            }
            search_idx = next_dot + 1;
        }

        false
    }

    pub fn len(&self) -> usize {
        self.domains.len()
    }

    pub fn is_empty(&self) -> bool {
        self.domains.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_matcher_exact_and_subdomains() {
        let list = vec![
            "discord.com".to_string(),
            "roblox.com".to_string(),
            ".steamcommunity.com".to_string(),
        ];
        let matcher = DomainMatcher::from_domains(&list);

        assert!(matcher.matches("discord.com"));
        assert!(matcher.matches("DISCORD.COM"));
        assert!(matcher.matches("gateway.discord.com"));
        assert!(matcher.matches("media.chat.gateway.discord.com"));

        assert!(matcher.matches("roblox.com"));
        assert!(matcher.matches("setup.roblox.com"));

        assert!(matcher.matches("steamcommunity.com"));
        assert!(matcher.matches("api.steamcommunity.com"));

        assert!(!matcher.matches("google.com"));
        assert!(!matcher.matches("notdiscord.com"));
        assert!(!matcher.matches("roblox.com.evil.org"));
    }
}
