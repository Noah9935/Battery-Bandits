-- Profilbild: Jedes Konto bekommt zufällig eines von 100 Waschbär-Bildern zugewiesen (Nummer 0 bis 99). Die
-- Vorgabe wird für jede Zeile einzeln gewürfelt, auch für vorhandene Konten.
alter table users add column avatar smallint not null default floor(random() * 100)::smallint check (avatar between 0 and 99);

-- Hat die Person ihr Passwort selbst geändert, überschreibt die Konfiguration (BB_USERS) es beim Start nicht mehr.
alter table users add column password_changed_at timestamptz;

-- Weiterleitung per WhatsApp ohne Anbieter-Vertrag: Jede Person hinterlegt den Schlüssel ihres persönlichen CallMeBot.
alter table notification_rules add column whatsapp_key text not null default '';
