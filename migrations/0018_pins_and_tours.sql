-- Merkliste und Touren liegen jetzt auf dem Server statt im Browser: Jede Person hat ihre eigene Merkliste,
-- Touren lassen sich anderen Konten freigeben, damit sie die Route abgehen.

-- Die persönliche Merkliste: Nodes, bei denen man die Batterie wechseln will.
create table user_pins (
    username   text not null,
    device_id  text not null references devices (id) on delete cascade,
    created_at timestamptz not null default now(),
    primary key (username, device_id)
);

-- Eine benannte Tour mit ihren Nodes. Der Name ist je Person eindeutig (ohne Groß- und Kleinschreibung).
create table tours (
    id         bigserial primary key,
    name       text not null check (char_length(btrim(name)) between 1 and 60),
    owner      text not null,
    device_ids text[] not null default '{}',
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);
create unique index tours_owner_name_idx on tours (owner, lower(btrim(name)));

-- Mit wem eine Tour geteilt ist. Wer hier steht, sieht die Tour und kann ihre Route öffnen.
create table tour_shares (
    tour_id    bigint not null references tours (id) on delete cascade,
    username   text not null,
    created_at timestamptz not null default now(),
    primary key (tour_id, username)
);
create index tour_shares_user_idx on tour_shares (username);

-- Neue Art im Postfach: "Noah hat eine Tour mit dir geteilt".
alter table inbox_items drop constraint inbox_items_kind_check;
alter table inbox_items add constraint inbox_items_kind_check check (kind in ('alert', 'assigned', 'comment', 'shared'));
