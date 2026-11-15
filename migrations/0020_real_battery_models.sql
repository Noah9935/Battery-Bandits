-- Der Batteriekatalog bekommt Modelle, wie sie als Pufferbatterie bei Ampeln und anderen Notstrom-Anwendungen in Frage
-- kommen: 12-V-AGM-Batterien der Klasse 80 bis 100 Ah. Die Preise sind Händlerpreise (brutto), Stand November 2026,
-- bei Händlern und Herstellern nachgeschlagen. Kein Hersteller nennt diese Typen ausdrücklich für Ampeln, es sind die
-- gängigen Typen der Klasse. Wer den Katalog pflegt, kann Preise und Einträge in der Oberfläche jederzeit ändern.
-- Der Simulator baut sie den erfundenen Geräten ein (Operations-Dienst, `SIMULATION_BATTERIES`). Vorhandene Einträge
-- gleichen Namens bleiben, wie sie sind.
insert into battery_types (name, price_cents, note, created_by_name)
select v.name, v.price_cents, v.note, 'Katalog (Recherche)'
from (values
    ('Effekta BTL 12-80 (AGM, 12 V, 80 Ah)', 15118,
     'Wartungsfreie AGM-Batterie für USV und Notstrom, 80 Ah (20 Stunden), Herstellerangabe 10 Jahre Lebensdauer nach Eurobat. Händlerpreis, Stand November 2026.'),
    ('Effekta BTL 12-100 (AGM, 12 V, 100 Ah)', 17563,
     'Gleiche Reihe wie die BTL 12-80, aber 100 Ah. Händlerpreis, Stand November 2026.'),
    ('Victron AGM Deep Cycle 12 V 90 Ah', 21262,
     'AGM Deep Cycle (zyklenfest), 12 V, 90 Ah. Händlerpreis, Stand November 2026.'),
    ('Dometic NDS GreenPower GP80 (AGM, 12 V, 80 Ah)', 25500,
     'AGM, 12 V, 80 Ah. Laut Hersteller 4 bis 7 Jahre Lebensdauer und bis zu 1200 Zyklen. Preis laut Herstellerseite, Stand November 2026.'),
    ('Electronicx AGM Marine 80 Ah (12 V)', 11599,
     'AGM, 12 V, 80 Ah, zyklenfest und wartungsfrei laut Anbieter. Preis laut Preisvergleich, Stand November 2026.')
) as v(name, price_cents, note)
where not exists (
    select 1 from battery_types t
    where lower(btrim(t.name)) = lower(btrim(v.name)) and t.archived_at is null
);
