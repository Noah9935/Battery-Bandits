-- Herkunft der Daten je Gerät: vom Dozenten (Messdatenplattform, Vorlesung) oder erfunden (Simulator).
-- Vorhandene Geräte stammen aus echten Quellen.
create type data_source as enum ('dozent', 'simulation');
alter table devices add column data_source data_source not null default 'dozent';
