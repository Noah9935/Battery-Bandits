-- Konten und Sitzungen. Passwörter nur als Argon2-Hash, Sitzungs-Token nur als SHA-256-Hash.
create table users (
    id            bigserial primary key,
    username      text not null unique check (username = lower(username) and length(username) between 2 and 32),
    display_name  text not null check (length(display_name) between 1 and 80),
    password_hash text not null,
    created_at    timestamptz not null default now()
);

create table sessions (
    token_hash text primary key,
    user_id    bigint not null references users (id) on delete cascade,
    created_at timestamptz not null default now(),
    expires_at timestamptz not null
);
create index sessions_user_idx on sessions (user_id);
create index sessions_expires_idx on sessions (expires_at);
