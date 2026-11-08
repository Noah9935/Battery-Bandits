-- Tickets: Art, Dringlichkeit, Zuständigkeit und Frist für Einsätze, dazu Kommentare und ein Aktivitätsprotokoll.
create type task_kind as enum ('battery', 'inspection', 'repair');
create type task_priority as enum ('low', 'normal', 'high');

alter table maintenance_tasks
    add column kind              task_kind     not null default 'battery',
    add column priority          task_priority not null default 'normal',
    add column assignee          text,
    add column assignee_name     text,
    add column due_date          date,
    add column created_by        text,
    add column created_by_name   text,
    add column completed_by_name text;
create index maintenance_assignee_idx on maintenance_tasks (assignee) where status = 'planned';

create table task_comments (
    id          bigserial primary key,
    task_id     bigint not null references maintenance_tasks (id) on delete cascade,
    author      text,
    author_name text not null,
    body        text not null check (length(body) between 1 and 2000),
    created_at  timestamptz not null default now()
);
create index task_comments_task_idx on task_comments (task_id, created_at);

-- Wer hat wann was getan (Planen, Zuweisen, Kommentieren, Erledigen, Abweisen).
create table activity_log (
    id         bigserial primary key,
    at         timestamptz not null default now(),
    actor      text,
    actor_name text not null,
    device_id  text,
    task_id    bigint,
    text       text not null
);
create index activity_log_at_idx on activity_log (at desc);
create index activity_log_device_idx on activity_log (device_id, at desc);
