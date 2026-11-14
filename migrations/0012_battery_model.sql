-- Modell der eingebauten Batterie (zum Beispiel "Varta Silver 12 V 80 Ah") und der Innenwiderstand je Zyklus.
-- Gleiche Modelle werden in der Effizienz zusammengefasst, um Kosten und Nutzen zu vergleichen.
alter table devices add column battery_model text check (battery_model is null or length(battery_model) between 1 and 60);
alter table battery_cycles add column model text;
alter table battery_cycles add column resistance_mohm real;
