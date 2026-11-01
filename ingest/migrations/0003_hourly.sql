-- no-transaction
-- Stundenwerte je Gerät (Continuous Aggregate). Das 30-Tage-Diagramm liest daraus
-- rund 720 statt 8.640 Punkte. Ein Continuous Aggregate lässt sich nicht innerhalb
-- einer Transaktion anlegen, deshalb die Anweisung oben.
create materialized view ingest.reading_hourly
with (timescaledb.continuous) as
select device_id,
       time_bucket(interval '1 hour', ts) as bucket,
       avg(charge_pct)                    as charge_pct
from ingest.reading
group by device_id, time_bucket(interval '1 hour', ts)
with no data;

select add_continuous_aggregate_policy('ingest.reading_hourly',
    start_offset      => interval '3 days',
    end_offset        => interval '1 hour',
    schedule_interval => interval '15 minutes');

-- Die Stundenwerte bleiben genauso lange erhalten wie die Rohwerte.
select add_retention_policy('ingest.reading_hourly', interval '12 months');
