-- Prognose der Restlaufzeit, vom Monitoring-Service fortgeschrieben.
alter table devices
    add column discharge_rate_pct_per_hour real,
    add column forecast_hours_remaining real;
