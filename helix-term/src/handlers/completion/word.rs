use std::{borrow::Cow, collections::HashMap, sync::Arc};

use helix_core::{
    self as core, chars::char_is_word, completion::CompletionProvider, movement, Transaction,
};
use helix_event::TaskHandle;
use helix_stdx::rope::RopeSliceExt as _;
use helix_view::{
    document::SavePoint, handlers::completion::ResponseContext, Document, DocumentId, Editor, ViewId,
};

use super::{request::TriggerKind, CompletionItem, CompletionItems, CompletionResponse, Trigger};

/// Fallback shown when a word's source buffer has no display name.
const COMPLETION_KIND: &str = "word";

pub(super) fn completion(
    editor: &Editor,
    trigger: Trigger,
    handle: TaskHandle,
    savepoint: Arc<SavePoint>,
) -> Option<impl FnOnce() -> CompletionResponse> {
    if !doc!(editor).word_completion_enabled() {
        return None;
    }
    let config = editor.config().word_completion;
    let doc_config = doc!(editor)
        .language_config()
        .and_then(|config| config.word_completion);
    let trigger_length = doc_config
        .and_then(|c| c.trigger_length)
        .unwrap_or(config.trigger_length)
        .get() as usize;

    let (view, doc) = current_ref!(editor);
    let rope = doc.text().clone();
    let word_index = editor.handlers.word_index().clone();
    let text = doc.text().slice(..);
    let selection = doc.selection(view.id).clone();
    let pos = selection.primary().cursor(text);
    let current_doc = doc.id();

    let cursor = movement::move_prev_word_start(text, core::Range::point(pos), 1);
    if cursor.head == pos {
        return None;
    }
    if trigger.kind != TriggerKind::Manual
        && text
            .slice(cursor.head..)
            .graphemes()
            .take(trigger_length)
            .take_while(|g| g.chars().all(char_is_word))
            .count()
            != trigger_length
    {
        return None;
    }

    let typed_word_range = cursor.head..pos;
    let typed_word = text.slice(typed_word_range.clone());
    let edit_diff = if typed_word
        .char(typed_word.len_chars().saturating_sub(1))
        .is_whitespace()
    {
        0
    } else {
        typed_word.len_chars()
    };

    if handle.is_canceled() {
        return None;
    }

    // A snapshot of file names for every open document, so the worker thread can label each word
    // with the buffer it came from without needing access to the `Editor`. Only the file name is
    // kept (not the full path) to keep the completion menu narrow. Built only once we're past the
    // early-return gates above, so bailed-out triggers don't pay for it.
    let doc_names: HashMap<DocumentId, String> = editor
        .documents()
        .map(|doc| {
            let name = doc
                .path()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| doc.display_name().into_owned());
            (doc.id(), name)
        })
        .collect();

    let future = move || {
        let text = rope.slice(..);
        let typed_word: Cow<_> = text.slice(typed_word_range).into();
        let items = word_index
            .matches(&typed_word)
            .into_iter()
            .filter(|m| m.word != typed_word.as_ref())
            .map(|m| {
                let transaction = Transaction::change_by_selection(&rope, &selection, |range| {
                    let cursor = range.cursor(text);
                    (cursor - edit_diff, cursor, Some((m.word.as_str()).into()))
                });
                CompletionItem::Other(core::CompletionItem {
                    transaction,
                    label: m.word.into(),
                    kind: source_label(&m.sources, current_doc, &doc_names),
                    documentation: None,
                    provider: CompletionProvider::Word,
                })
            })
            .collect();

        CompletionResponse {
            items: CompletionItems::Other(items),
            provider: CompletionProvider::Word,
            context: ResponseContext {
                is_incomplete: false,
                priority: 0,
                savepoint,
            },
        }
    };

    Some(future)
}

/// Builds the `kind` column shown next to a word completion: the source buffer of the word.
///
/// When the word lives in the current buffer that name is shown, otherwise an arbitrary source
/// buffer is chosen. Words present in several buffers are not distinguished.
fn source_label(
    sources: &[DocumentId],
    current_doc: DocumentId,
    doc_names: &HashMap<DocumentId, String>,
) -> Cow<'static, str> {
    // Prefer the current buffer so the most relevant source is shown first.
    let primary = sources
        .iter()
        .find(|&&doc| doc == current_doc)
        .or_else(|| sources.first());

    let Some(&primary) = primary else {
        return Cow::Borrowed(COMPLETION_KIND);
    };

    match doc_names.get(&primary) {
        Some(name) => Cow::Owned(name.clone()),
        None => Cow::Borrowed(COMPLETION_KIND),
    }
}

pub(super) fn retain_valid_completions(
    trigger: Trigger,
    doc: &Document,
    view_id: ViewId,
    items: &mut Vec<CompletionItem>,
) {
    if trigger.kind == TriggerKind::Manual {
        return;
    }

    let text = doc.text().slice(..);
    let cursor = doc.selection(view_id).primary().cursor(text);
    if text
        .get_char(cursor.saturating_sub(1))
        .is_some_and(|ch| ch.is_whitespace())
    {
        items.retain(|item| {
            !matches!(
                item,
                CompletionItem::Other(core::CompletionItem {
                    provider: CompletionProvider::Word,
                    ..
                })
            )
        });
    }
}
