use super::*;
use crate::settings::calendar::Message as Account;
use crate::settings::calendar::system::{Message as Picked, Picker};

impl Settings {
    /// Calendars from Online Accounts and other EDS sources, each with a
    /// switch that adds it to the calendar list or takes it out.
    pub(super) fn calendar_system_picker(&self) -> Element<'_, Message> {
        let editor = &self.calendar;
        let message = |value| Message::Calendar(Account::System(value));
        let mut rows = vec![label(
            "Calendars from accounts in Online Accounts, such as Google and Microsoft 365.",
            12.,
            MUTED,
        )];
        match &editor.system.listed {
            None => rows.push(label("Looking for calendars…", 13., MUTED)),
            Some(Err(problem)) => rows.push(detail_state(
                problem.to_string(),
                "Evolution Data Server keeps calendars from Online Accounts. Your other calendars still work.",
            )),
            Some(Ok(listed)) if listed.is_empty() => rows.push(detail_state(
                "No system calendars yet",
                "Add an account with a calendar in Online Accounts, then open this list again.",
            )),
            Some(Ok(listed)) => {
                let full = Picker::full(editor);
                for (index, calendar) in listed.iter().enumerate() {
                    let on = Picker::added(editor, calendar);
                    let uid = calendar.uid.clone();
                    let switch = toggler(on).on_toggle_maybe(
                        (!editor.busy && (on || !full))
                            .then(|| move |on| message(Picked::Toggle(uid.clone(), on))),
                    );
                    rows.push(field(
                        Some(calendar.name.clone()),
                        Some(on),
                        detail_state(calendar.name.clone(), calendar.account.clone()),
                        container(switch)
                            .id(cosmic::widget::Id::new(format!("calendar-system-{index}")))
                            .into(),
                    ));
                }
                if full {
                    rows.push(label(
                        "Eight calendars are shown. Remove one to add another.",
                        12.,
                        MUTED,
                    ));
                }
            }
        }
        let mut actions = vec![];
        if editor.system.online_accounts {
            actions.push(slot(
                button::standard("Online Accounts…")
                    .id(cosmic::widget::Id::new("calendar-online-accounts"))
                    .on_press(message(Picked::OnlineAccounts))
                    .into(),
            ));
        }
        actions.push(slot(
            button::text("Done")
                .id(cosmic::widget::Id::new("calendar-system-done"))
                .on_press(message(Picked::Open(false)))
                .into(),
        ));
        rows.push(row::with_children(actions).spacing(8).wrap().into());
        container(column::with_children(rows).spacing(8))
            .id("calendar-system-picker")
            .width(Length::Fill)
            .into()
    }
}
