-- Ältere Firmware (zum Beispiel Gerät 101 in der Historie der Messdatenplattform) sendet keine Temperatur.
alter table readings alter column temperature_c drop not null;
