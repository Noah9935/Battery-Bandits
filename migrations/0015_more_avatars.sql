-- Mehr Profilbilder: Es gibt jetzt 164 Waschbär-Fotos (Nummer 0 bis 163). Vorhandene Konten behalten ihr Bild, neue Konten
-- bekommen zufällig eines der 164 zugewiesen. In den Optionen lässt sich das Bild selbst wählen.
alter table users drop constraint users_avatar_check;
alter table users add constraint users_avatar_check check (avatar between 0 and 163);
alter table users alter column avatar set default floor(random() * 164)::smallint;
