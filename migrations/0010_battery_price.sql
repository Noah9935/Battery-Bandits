-- Preis der Batterie in Cent. Wer einen Wechsel einträgt, kann ihn angeben. Er gehört zur eingebauten Batterie
-- (devices) und wandert beim nächsten Wechsel zusammen mit dem Zyklus in battery_cycles.
alter table devices add column battery_price_cents integer check (battery_price_cents >= 0);
alter table battery_cycles add column price_cents integer check (price_cents >= 0);
