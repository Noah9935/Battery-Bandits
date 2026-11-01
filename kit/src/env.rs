//! Konfiguration aus Umgebungsvariablen (NF07: keine Codeänderung nötig).

use std::str::FromStr;

use anyhow::{Context, Result, anyhow};

/// Pflichtwert. Fehlt er, bricht der Dienst mit einer klaren Meldung ab.
pub fn required(name: &str) -> Result<String> {
    std::env::var(name)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| anyhow!("Umgebungsvariable {name} fehlt"))
}

/// Textwert mit Vorgabe.
pub fn or(name: &str, default: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.to_owned())
}

/// Wert beliebigen Typs mit Vorgabe. Ein vorhandener, aber unlesbarer Wert ist ein Fehler.
pub fn parse_or<T>(name: &str, default: T) -> Result<T>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    match std::env::var(name).ok().filter(|v| !v.trim().is_empty()) {
        None => Ok(default),
        Some(v) => v
            .trim()
            .parse::<T>()
            .with_context(|| format!("Umgebungsvariable {name} ist ungültig: {v}")),
    }
}

/// Wahrheitswert: `1`, `true`, `yes`, `on` (ohne Beachtung der Groß- und Kleinschreibung).
pub fn flag(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(v) => matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"),
        Err(_) => default,
    }
}
