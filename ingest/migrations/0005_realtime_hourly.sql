-- Echtzeit-Aggregation: Abfragen auf die Stundenwerte enthalten auch die Zeit, die der
-- Zeitplan noch nicht verdichtet hat. Ohne diese Einstellung wären Monats- und
-- Jahresansicht bis zum ersten Lauf der Richtlinie leer und stets eine Viertelstunde alt.
alter materialized view ingest.reading_hourly set (timescaledb.materialized_only = false);
