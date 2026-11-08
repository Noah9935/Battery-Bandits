//! Einsätze als Tickets: Liste mit Filter, Detaildialog mit Zuständigkeit und Kommentaren.

use chrono::NaiveDate;
use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::{MaintenanceStatus, MaintenanceTask, TaskComment, TaskKind, TaskPriority};

use crate::state::{ModalKind, Store};
use crate::{api, format, jsutil};

/// Wessen Einsätze die Liste zeigt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Who {
    All,
    Mine,
    Free,
}

impl Who {
    pub const ALL: [Who; 3] = [Who::All, Who::Mine, Who::Free];

    pub fn label(self) -> &'static str {
        match self {
            Who::All => "Alle",
            Who::Mine => "Meine",
            Who::Free => "Ohne Zuständigkeit",
        }
    }

    pub fn matches(self, task: &MaintenanceTask, me: Option<&str>) -> bool {
        match self {
            Who::All => true,
            Who::Mine => me.is_some() && task.assignee.as_deref() == me,
            Who::Free => task.assignee.is_none(),
        }
    }
}

/// Text zur Frist und ob sie überschritten ist.
pub fn due_text(due: NaiveDate, today: NaiveDate) -> (String, bool) {
    let days = (due - today).num_days();
    let date = due.format("%d.%m.%Y");
    match days {
        d if d < -1 => (format!("seit {} Tagen überfällig ({date})", -d), true),
        -1 => (format!("seit gestern überfällig ({date})"), true),
        0 => ("heute fällig".to_owned(), false),
        1 => ("morgen fällig".to_owned(), false),
        d => (format!("fällig in {d} Tagen ({date})"), false),
    }
}

pub fn assignee_text(task: &MaintenanceTask) -> String {
    match task.assignee_name.as_deref().or(task.assignee.as_deref()) {
        Some(name) => name.to_owned(),
        None => "Niemand zuständig".to_owned(),
    }
}

pub fn priority_class(priority: TaskPriority) -> &'static str {
    match priority {
        TaskPriority::High => "origin prio-high",
        TaskPriority::Normal => "origin prio-normal",
        TaskPriority::Low => "origin prio-low",
    }
}

/// Zahl der Einsätze, die dieser Person zugewiesen und noch offen sind.
pub fn open_for(tasks: &[MaintenanceTask], username: &str) -> usize {
    tasks
        .iter()
        .filter(|t| t.status == MaintenanceStatus::Planned && t.assignee.as_deref() == Some(username))
        .count()
}

/// Schaltet die Zuständigkeit und meldet das Ergebnis.
pub fn assign(s: Store, task_id: i64, assignee: Option<String>, done: &'static str) {
    spawn_local(async move {
        match api::assign_task(task_id, assignee).await {
            Ok(_) => s.say(done),
            Err(e) => s.say(e),
        }
        s.refresh_soon();
    });
}

pub fn claim(s: Store, task_id: i64) {
    if let Some(me) = s.me.get_untracked() {
        assign(s, task_id, Some(me.username), "Du kümmerst dich jetzt darum.");
    }
}

#[component]
pub fn TicketRow(task: MaintenanceTask) -> impl IntoView {
    let s = expect_context::<Store>();
    let device_id = task.device_id.clone();
    let title = {
        let device_id = device_id.clone();
        move || {
            format::device_title(
                &device_id,
                s.device(&device_id).as_ref().and_then(|d| d.name.as_deref()),
            )
        }
    };
    let task_id = task.id;
    let done = task.status == MaintenanceStatus::Completed;
    let free = task.assignee.is_none() && !done;
    let due = task.due_date.filter(|_| !done).map(|d| due_text(d, jsutil::today()));
    view! {
        <li class="task-row ticket">
            <button class="ticket-main" on:click=move |_| s.modal.set(Some(ModalKind::Ticket { task_id }))>
                <span class="ticket-title">
                    <span class="origin">{task.kind.label()}</span>
                    {(task.priority == TaskPriority::High && !done)
                        .then(|| view! { <span class=priority_class(task.priority)>"dringend"</span> })}
                    <strong>{title}</strong>
                </span>
                <span class="ticket-meta muted">
                    <span>{assignee_text(&task)}</span>
                    {due
                        .map(|(text, late)| view! { <span class=if late { "late" } else { "" }>{text}</span> })}
                    {(task.comment_count > 0)
                        .then(|| view! { <span>{format!("{} Kommentare", task.comment_count)}</span> })}
                    {done
                        .then(|| {
                            format!(
                                "Erledigt am {}{}",
                                task.completed_at.map(jsutil::date_label).unwrap_or_default(),
                                task.completed_by_name.as_deref().map(|n| format!(" von {n}")).unwrap_or_default()
                            )
                        })}
                </span>
                {task.note.clone().map(|n| view! { <span class="note">{n}</span> })}
            </button>
            <div class="ticket-actions">
            {free
                .then(|| {
                    view! {
                        <button class="btn" on:click=move |_| claim(s, task_id)>"Ich kümmere mich darum"</button>
                    }
                })}
            {(!done)
                .then(|| {
                    let device_id = device_id.clone();
                    view! {
                        <button
                            class="btn primary"
                            on:click=move |_| {
                                s.modal
                                    .set(
                                        Some(ModalKind::Complete {
                                            task_id,
                                            device_id: device_id.clone(),
                                        }),
                                    )
                            }
                        >
                            {if task.kind == TaskKind::Battery { "Batterie gewechselt" } else { "Erledigt" }}
                        </button>
                    }
                })}
            </div>
        </li>
    }
}

#[component]
pub fn Tickets() -> impl IntoView {
    let s = expect_context::<Store>();
    let who = RwSignal::new(Who::All);
    let me = move || s.me.with(|m| m.as_ref().map(|m| m.username.clone()));
    let planned = move || {
        s.tasks.with(|tasks| {
            let me = me();
            tasks
                .iter()
                .filter(|t| t.status == MaintenanceStatus::Planned && who.get().matches(t, me.as_deref()))
                .cloned()
                .collect::<Vec<_>>()
        })
    };
    let completed = move || {
        s.tasks.with(|tasks| {
            tasks
                .iter()
                .filter(|t| t.status == MaintenanceStatus::Completed)
                .take(10)
                .cloned()
                .collect::<Vec<_>>()
        })
    };
    let key = |t: &MaintenanceTask| {
        (
            t.id,
            t.status,
            t.assignee.clone(),
            t.comment_count,
            t.priority,
            t.due_date,
        )
    };
    view! {
        <section class="panel padded">
            <div class="panel-head">
                <h2>"Einsätze"</h2>
                <div class="seg" role="group" aria-label="Einsätze filtern">
                    {Who::ALL
                        .into_iter()
                        .map(|w| {
                            view! {
                                <button
                                    class=move || if who.get() == w { "seg-btn active" } else { "seg-btn" }
                                    on:click=move |_| who.set(w)
                                >
                                    {move || {
                                        match (w, me()) {
                                            (Who::Mine, Some(name)) => {
                                                format!("Meine ({})", s.tasks.with(|t| open_for(t, &name)))
                                            }
                                            _ => w.label().to_owned(),
                                        }
                                    }}
                                </button>
                            }
                        })
                        .collect_view()}
                </div>
            </div>
            {move || {
                let list = planned();
                if list.is_empty() {
                    let text = match who.get() {
                        Who::All => "Keine offenen Einsätze. Einsätze lassen sich in der Übersicht oder in den Gerätedetails planen.",
                        Who::Mine => "Dir ist gerade nichts zugewiesen.",
                        Who::Free => "Alle offenen Einsätze haben jemanden, der sich kümmert.",
                    };
                    view! { <p class="muted">{text}</p> }.into_any()
                } else {
                    view! {
                        <ul class="tasks">
                            <For each=planned key=key children=move |t| view! { <TicketRow task=t/> }/>
                        </ul>
                    }
                        .into_any()
                }
            }}
            {move || {
                (!completed().is_empty())
                    .then(|| {
                        view! {
                            <h3>"Zuletzt erledigt"</h3>
                            <ul class="tasks">
                                <For each=completed key=key children=move |t| view! { <TicketRow task=t/> }/>
                            </ul>
                        }
                    })
            }}
        </section>
    }
}

/// Detaildialog: Angaben, Zuständigkeit, Kommentare.
#[component]
pub fn TicketDialog(task_id: i64) -> impl IntoView {
    let s = expect_context::<Store>();
    let task = move || s.tasks.with(|t| t.iter().find(|t| t.id == task_id).cloned());
    let comments = RwSignal::new(Vec::<TaskComment>::new());
    let draft = RwSignal::new(String::new());
    let problem = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

    // Kommentare laden, sobald sich die Zahl ändert (auch durch andere Personen).
    Effect::new(move |_| {
        let _count = task().map(|t| t.comment_count);
        spawn_local(async move {
            if let Ok(list) = api::comments(task_id).await {
                comments.set(list);
            }
        });
    });

    let send = move |_| {
        let text = draft.get_untracked();
        if text.trim().is_empty() {
            problem.set(Some("Bitte zuerst etwas schreiben.".to_owned()));
            return;
        }
        busy.set(true);
        spawn_local(async move {
            match api::add_comment(task_id, &text).await {
                Ok(c) => {
                    comments.update(|list| list.push(c));
                    draft.set(String::new());
                    problem.set(None);
                    s.refresh_soon();
                }
                Err(e) => problem.set(Some(e)),
            }
            busy.set(false);
        });
    };

    view! {
        {move || match task() {
            None => view! { <h2>"Einsatz nicht gefunden"</h2><p class="muted">"Vielleicht wurde er inzwischen gelöscht."</p> }.into_any(),
            Some(t) => {
                let done = t.status == MaintenanceStatus::Completed;
                let device_id = t.device_id.clone();
                let title = format::device_title(&t.device_id, s.device(&t.device_id).as_ref().and_then(|d| d.name.as_deref()));
                let due = t.due_date.map(|d| due_text(d, jsutil::today()));
                let mine = s.me.with(|m| m.as_ref().map(|m| m.username.clone())) == t.assignee;
                let current = t.assignee.clone().unwrap_or_default();
                view! {
                    <h2>{t.kind.label()}</h2>
                    <p class="muted">{title}</p>
                    <dl class="kv-list">
                        <div class="kv"><dt>"Status"</dt><dd>{if done { "Erledigt" } else { "Offen" }}</dd></div>
                        <div class="kv"><dt>"Dringlichkeit"</dt><dd>{t.priority.label()}</dd></div>
                        {due
                            .map(|(text, late)| {
                                view! { <div class="kv"><dt>"Frist"</dt><dd class=if late && !done { "late" } else { "" }>{text}</dd></div> }
                            })}
                        <div class="kv">
                            <dt>"Angelegt"</dt>
                            <dd>
                                {format!(
                                    "{}{}",
                                    jsutil::date_label(t.created_at),
                                    t.created_by_name.as_deref().map(|n| format!(" von {n}")).unwrap_or_default()
                                )}
                            </dd>
                        </div>
                        {t.completed_at
                            .map(|at| {
                                view! {
                                    <div class="kv">
                                        <dt>"Erledigt"</dt>
                                        <dd>
                                            {format!(
                                                "{}{}",
                                                jsutil::date_label(at),
                                                t.completed_by_name.as_deref().map(|n| format!(" von {n}")).unwrap_or_default()
                                            )}
                                        </dd>
                                    </div>
                                }
                            })}
                    </dl>
                    {t.note.clone().map(|n| view! { <p class="note">{n}</p> })}
                    {(!done)
                        .then(|| {
                            let id = t.id;
                            view! {
                                <label class="field modal-field">
                                    <span>"Zuständig"</span>
                                    <select
                                        on:change=move |ev| {
                                            let value = event_target_value(&ev);
                                            assign(
                                                s,
                                                id,
                                                Some(value).filter(|v| !v.is_empty()),
                                                "Zuständigkeit geändert.",
                                            );
                                        }
                                    >
                                        <option value="" selected=current.is_empty()>"Niemand"</option>
                                        {s.users
                                            .get()
                                            .into_iter()
                                            .map(|u| {
                                                let selected = u.username == current;
                                                view! { <option value=u.username.clone() selected=selected>{u.display_name}</option> }
                                            })
                                            .collect_view()}
                                    </select>
                                </label>
                                {(!mine)
                                    .then(|| {
                                        view! {
                                            <button class="btn" on:click=move |_| claim(s, id)>"Ich kümmere mich darum"</button>
                                        }
                                    })}
                            }
                        })}
                    <h3 class="modal-sub">"Kommentare"</h3>
                    <ul class="comments">
                        {move || {
                            let list = comments.get();
                            if list.is_empty() {
                                view! { <li class="muted">"Noch keine Kommentare."</li> }.into_any()
                            } else {
                                list.into_iter()
                                    .map(|c| {
                                        view! {
                                            <li>
                                                <span class="comment-head">
                                                    <strong>{c.author_name}</strong>
                                                    <span class="muted">{jsutil::date_time_label(c.created_at)}</span>
                                                </span>
                                                <span class="comment-body">{c.body}</span>
                                            </li>
                                        }
                                    })
                                    .collect_view()
                                    .into_any()
                            }
                        }}
                    </ul>
                    <label class="field modal-field">
                        <span>"Neuer Kommentar"</span>
                        <textarea
                            rows="2"
                            maxlength="2000"
                            placeholder="Zum Beispiel: Schlüssel liegt beim Hausmeister"
                            prop:value=move || draft.get()
                            on:input=move |ev| draft.set(event_target_value(&ev))
                        ></textarea>
                    </label>
                    {move || problem.get().map(|p| view! { <p class="form-error" role="alert">{p}</p> })}
                    <div class="modal-actions">
                        <button class="btn" on:click=move |_| s.modal.set(None)>"Schließen"</button>
                        <button class="btn" disabled=move || busy.get() on:click=send>"Kommentar senden"</button>
                        {(!done)
                            .then(|| {
                                let id = t.id;
                                let device_id = device_id.clone();
                                view! {
                                    <button
                                        class="btn primary"
                                        on:click=move |_| {
                                            s.modal
                                                .set(
                                                    Some(ModalKind::Complete {
                                                        task_id: id,
                                                        device_id: device_id.clone(),
                                                    }),
                                                )
                                        }
                                    >
                                        {if t.kind == TaskKind::Battery { "Batterie gewechselt" } else { "Erledigt" }}
                                    </button>
                                }
                            })}
                    </div>
                }
                    .into_any()
            }
        }}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    fn task(assignee: Option<&str>, status: MaintenanceStatus) -> MaintenanceTask {
        MaintenanceTask {
            id: 1,
            device_id: "TN0001".into(),
            alert_id: None,
            status,
            note: None,
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
            completed_at: None,
            kind: TaskKind::Battery,
            priority: TaskPriority::Normal,
            assignee: assignee.map(str::to_owned),
            assignee_name: assignee.map(|a| a.to_uppercase()),
            due_date: None,
            created_by_name: None,
            completed_by_name: None,
            comment_count: 0,
        }
    }

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
    }

    #[test]
    fn deadlines_are_worded_by_distance() {
        assert_eq!(due_text(day(10), day(10)), ("heute fällig".to_owned(), false));
        assert_eq!(due_text(day(11), day(10)), ("morgen fällig".to_owned(), false));
        assert_eq!(
            due_text(day(14), day(10)),
            ("fällig in 4 Tagen (14.11.2026)".to_owned(), false)
        );
        assert_eq!(
            due_text(day(9), day(10)),
            ("seit gestern überfällig (09.11.2026)".to_owned(), true)
        );
        assert_eq!(
            due_text(day(5), day(10)),
            ("seit 5 Tagen überfällig (05.11.2026)".to_owned(), true)
        );
    }

    #[test]
    fn the_filter_knows_mine_and_free() {
        let mine = task(Some("noah"), MaintenanceStatus::Planned);
        let theirs = task(Some("fynn"), MaintenanceStatus::Planned);
        let free = task(None, MaintenanceStatus::Planned);
        assert!(Who::All.matches(&mine, Some("noah")) && Who::All.matches(&free, None));
        assert!(Who::Mine.matches(&mine, Some("noah")));
        assert!(!Who::Mine.matches(&theirs, Some("noah")));
        assert!(!Who::Mine.matches(&free, Some("noah")));
        assert!(!Who::Mine.matches(&mine, None), "ohne Anmeldung gibt es keine eigenen");
        assert!(Who::Free.matches(&free, Some("noah")) && !Who::Free.matches(&mine, Some("noah")));
    }

    #[test]
    fn assignee_text_prefers_the_display_name() {
        assert_eq!(assignee_text(&task(Some("noah"), MaintenanceStatus::Planned)), "NOAH");
        assert_eq!(
            assignee_text(&task(None, MaintenanceStatus::Planned)),
            "Niemand zuständig"
        );
    }

    #[test]
    fn open_tickets_are_counted_per_person() {
        let tasks = vec![
            task(Some("noah"), MaintenanceStatus::Planned),
            task(Some("noah"), MaintenanceStatus::Completed),
            task(Some("fynn"), MaintenanceStatus::Planned),
        ];
        assert_eq!(open_for(&tasks, "noah"), 1);
        assert_eq!(open_for(&tasks, "niemand"), 0);
    }
}
