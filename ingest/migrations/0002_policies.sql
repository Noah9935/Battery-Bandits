-- Kompression nach sieben Tagen, gruppiert nach Gerät.
alter table ingest.reading set (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'device_id',
    timescaledb.compress_orderby = 'ts desc'
);
select add_compression_policy('ingest.reading', interval '7 days');

-- Aufbewahrung: 12 Monate (NF08). Die Dauer ist mit dem Auftraggeber noch zu bestätigen
-- und lässt sich mit remove_retention_policy und add_retention_policy ändern.
select add_retention_policy('ingest.reading', interval '12 months');
