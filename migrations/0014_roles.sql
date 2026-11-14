-- Rolle eines Kontos: Admins legen Konten an, setzen Passwörter zurück und vergeben die Rolle.
alter table users add column role text not null default 'user' check (role in ('admin', 'user'));
