//! Personal notes on an asset (NoteCard.tsx): why I bought, my plan, when I sell. Kept in this browser only
//! ("altim.notes.v1", `altim_core::web::decision::notes`), saved when the field loses the focus.
use altim_core::web::decision::notes::{NOTE_MAX, NOTES_KEY, note, save};
use yew::prelude::*;

use crate::state::{local_get, local_set, now};

#[derive(Properties, PartialEq)]
pub struct NoteCardProps {
    /// "crypto:BTC"
    pub id: AttrValue,
    pub symbol: AttrValue,
}

#[component]
pub fn NoteCard(p: &NoteCardProps) -> Html {
    let text = {
        let id = p.id.clone();
        use_state(move || note(local_get(NOTES_KEY).as_deref(), &id).text)
    };
    let updated = {
        let id = p.id.clone();
        use_state(move || note(local_get(NOTES_KEY).as_deref(), &id).updated)
    };
    {
        let (text, updated) = (text.clone(), updated.clone());
        use_effect_with(p.id.clone(), move |id| {
            let n = note(local_get(NOTES_KEY).as_deref(), id);
            text.set(n.text);
            updated.set(n.updated);
        });
    }
    let on_input = {
        let text = text.clone();
        Callback::from(move |e: InputEvent| text.set(super::why_card::event_value(&e)))
    };
    let on_blur = {
        let (text, updated, id) = (text.clone(), updated.clone(), p.id.clone());
        Callback::from(move |_: FocusEvent| {
            if crate::state::local_storage().is_none() {
                return;
            }
            let t = now();
            let (stored, kept) = save(local_get(NOTES_KEY).as_deref(), &id, &text, t);
            local_set(NOTES_KEY, &stored);
            // A refused write (private browsing) is not kept: the date is not shown.
            if local_get(NOTES_KEY).as_deref() == Some(stored.as_str()) {
                updated.set(kept.then_some(t));
            }
        })
    };
    let saved = updated.map(|u| format!("Enregistré le {}. ", crate::ui::fr_date_time_short(u))).unwrap_or_default();
    html! {
        <div class="card note-card">
            <h2 class="card-title">{ format!("Mes notes · {}", p.symbol) }</h2>
            <textarea
                value={(*text).clone()}
                maxlength={NOTE_MAX.to_string()}
                rows="4"
                placeholder="Pourquoi j'achète, mon plan, quand je vends… (ex. « acheté pour 3 ans, je renforce sous 60 000 $, je vends si la thèse change »)"
                oninput={on_input}
                onblur={on_blur}
                aria-label={format!("Mes notes sur {}", p.symbol)}
            />
            <p class="muted small">
                { saved }
                { "Gardé dans ce navigateur seulement, jamais envoyé au serveur. Relire sa thèse avant d'acheter ou de vendre évite les décisions sur un coup de tête." }
            </p>
        </div>
    }
}
