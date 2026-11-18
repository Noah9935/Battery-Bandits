-- Ladezustand mit Fehlerangabe.
--
-- Der Erfassungs-Service schätzt den Ladezustand mit einem Kalman-Filter aus Ladungsintegration und Ruhespannung
-- (`shared::estimator`). Zu jedem Wert gehört eine Standardabweichung in Prozentpunkten. Der Zustand des Filters
-- (Offset des Stromsensors, Kapazitätsfaktor, Kovarianz, gleitendes Mittel des Stroms) liegt je Gerät als JSON in der
-- Datenbank, damit er einen Neustart überlebt. Ohne Zustand beginnt das Filter bei der nächsten Messung neu aus der
-- Spannung. Das betrifft einmalig alle vorhandenen Geräte und ersetzt ihren bisherigen, ungeprüften Wert.
--
-- Die Prognose der Restlaufzeit bekommt eine Spanne: vorsichtige und optimistische Grenze in Stunden (80 Prozent).

alter table devices
    add column soc_sigma_percent  real check (soc_sigma_percent is null or soc_sigma_percent >= 0),
    add column estimator_state    jsonb,
    add column forecast_hours_low  real,
    add column forecast_hours_high real;

alter table readings add column soc_sigma_percent real;
