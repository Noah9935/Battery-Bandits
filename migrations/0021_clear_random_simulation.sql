-- Aufräumen: Die erfundenen Geräte kommen jetzt vom Simulator über MQTT (50 feste Nodes SIM01 bis SIM50, so gebaut wie
-- die Nodes der Vorlesung). Was vorher an Zufallsdaten für erfundene Geräte entstanden ist (Beispiel-Einsätze mit
-- Kommentaren, weitere Geräte bis SIM60, alte Messreihen), wird einmalig entfernt. Der Simulator legt die Geräte mit
-- der nächsten Nachricht neu an. Echte Geräte (Herkunft "dozent") bleiben unberührt. Auf Prod gibt es keine erfundenen
-- Geräte, dort ändert diese Migration nichts.
delete from inbox_items where device_id in (select id from devices where data_source = 'simulation');
delete from activity_log where device_id in (select id from devices where data_source = 'simulation');

-- Touren führen ihre Geräte als Liste ohne Fremdschlüssel: erfundene Geräte dort austragen.
update tours
set device_ids = array(
    select x from unnest(device_ids) as x
    where x not in (select id from devices where data_source = 'simulation')
)
where device_ids && array(select id from devices where data_source = 'simulation');

-- Verworfene Nachrichten des Simulators (Fehlertests). Nachrichten echter Nodes bleiben.
delete from invalid_messages_log where topic like 'traffic-nodes/SIM%';

-- Messwerte, Meldungen, Einsätze samt Kommentaren, Batteriezyklen, Merklisten und Eintragungen hängen per
-- Fremdschlüssel am Gerät und verschwinden mit ihm.
delete from devices where data_source = 'simulation';
