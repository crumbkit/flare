use super::{Forms, Tied, Unusable};
use crate::secret;
use hangar_core::{BindingName, Blob, RecordPath, SlotName};
use std::time::{Duration, Instant};

const LIFE: Duration = Duration::from_secs(60 * 60);

fn tied(session: &str) -> Tied {
    Tied {
        session: secret::sha256(session.as_bytes()),
        binding: BindingName::try_from("kitchen").unwrap(),
        slot: SlotName::try_from("task").unwrap(),
        path: RecordPath::try_from("/tasks/tap").unwrap(),
        blob: Blob::try_from("0123456789abcdef0123456789abcdef01234567").unwrap(),
    }
}

#[test]
fn a_session_that_draws_many_forms_drops_only_its_own_oldest_tokens() {
    let forms = Forms::new(16, LIFE);
    let now = Instant::now();
    let kept = forms.issue(tied("reader"), now).unwrap();
    for _ in 0..100 {
        forms.issue(tied("other"), now).unwrap();
    }
    assert_eq!(forms.take(&kept, now).unwrap(), Some(tied("reader")));
}

#[test]
fn a_session_keeps_its_newest_tokens_when_it_passes_its_share() {
    let forms = Forms::new(16, LIFE);
    let now = Instant::now();
    let oldest = forms.issue(tied("reader"), now).unwrap();
    let mut newest = Vec::new();
    for _ in 0..forms.per_session {
        newest.push(forms.issue(tied("reader"), now).unwrap());
    }
    assert_eq!(forms.take(&oldest, now).unwrap(), None);
    for token in &newest {
        assert_eq!(forms.take(token, now).unwrap(), Some(tied("reader")));
    }
}

#[test]
fn a_table_that_a_panic_poisoned_answers_unusable() {
    let forms = Forms::new(16, LIFE);
    let now = Instant::now();
    let token = forms.issue(tied("reader"), now).unwrap();
    let poisoned = std::panic::catch_unwind(|| {
        let _rows = forms.rows.lock().unwrap();
        panic!("a panic while the table is locked");
    });
    assert!(poisoned.is_err());
    assert!(matches!(forms.take(&token, now), Err(Unusable)));
    assert!(forms.issue(tied("reader"), now).is_none());
}
