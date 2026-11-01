-- Protokoll verworfener Nachrichten (F06).
create table ingest.rejected_message (
    id          bigserial primary key,
    received_at timestamptz not null default now(),
    topic       text        not null,
    payload     text        not null,
    reason      text        not null
);

create index rejected_message_received_at_idx on ingest.rejected_message (received_at desc);
