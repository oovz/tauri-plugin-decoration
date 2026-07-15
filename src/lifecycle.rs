use std::{
    collections::HashMap,
    fmt,
    sync::{Mutex, MutexGuard},
    thread::ThreadId,
};

#[cfg(target_os = "macos")]
use crate::traffic_state::TrafficRegistry;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct Generation(u64);

impl Generation {
    pub(crate) fn get(self) -> u64 {
        self.0
    }

    #[cfg(test)]
    fn from_raw(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct DocumentToken(u64);

impl DocumentToken {
    pub(crate) fn get(self) -> u64 {
        self.0
    }

    #[cfg(test)]
    fn from_raw(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FrontendTarget {
    pub(crate) window: Generation,
    pub(crate) document: DocumentToken,
}

impl FrontendTarget {
    pub(crate) fn from_values(window: u64, document: u64) -> Option<Self> {
        (window != 0 && document != 0).then_some(Self {
            window: Generation(window),
            document: DocumentToken(document),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Preparing(DocumentToken),
    Applying(DocumentToken),
    Active(DocumentToken),
    Failed(DocumentToken),
    Native(DocumentToken),
    Destroying,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Entry {
    generation: Generation,
    phase: Phase,
}

impl Entry {
    fn target(self) -> Option<FrontendTarget> {
        let document = match self.phase {
            Phase::Preparing(document)
            | Phase::Applying(document)
            | Phase::Active(document)
            | Phase::Failed(document)
            | Phase::Native(document) => document,
            Phase::Destroying => return None,
        };
        Some(FrontendTarget {
            window: self.generation,
            document,
        })
    }

    fn matches(self, target: FrontendTarget) -> bool {
        self.target() == Some(target)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ActivationDecision {
    AlreadyActive(FrontendTarget),
    Reserved(FrontendTarget),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LifecycleError {
    ActivationInProgress(FrontendTarget),
}

impl fmt::Display for LifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ActivationInProgress(target) => write!(
                formatter,
                "decoration activation is already in progress for window generation {} and document {}",
                target.window.get(),
                target.document.get()
            ),
        }
    }
}

impl std::error::Error for LifecycleError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReadinessDecision {
    Apply,
    AlreadyApplying,
    AlreadyActive,
    Stale,
}

#[derive(Debug, Default)]
pub(crate) struct Lifecycle {
    next_generation: u64,
    next_document: u64,
    entries: HashMap<String, Entry>,
}

impl Lifecycle {
    fn allocate_window_target(&mut self) -> FrontendTarget {
        self.next_generation += 1;
        self.next_document += 1;
        FrontendTarget {
            window: Generation(self.next_generation),
            document: DocumentToken(self.next_document),
        }
    }

    fn allocate_document(&mut self, generation: Generation) -> FrontendTarget {
        self.next_document += 1;
        FrontendTarget {
            window: generation,
            document: DocumentToken(self.next_document),
        }
    }

    pub(crate) fn begin_activation(
        &mut self,
        label: &str,
    ) -> Result<ActivationDecision, LifecycleError> {
        if let Some(entry) = self.entries.get(label).copied() {
            match entry.phase {
                Phase::Active(_) => {
                    return Ok(ActivationDecision::AlreadyActive(
                        entry.target().expect("active entry has a document"),
                    ));
                }
                Phase::Preparing(_) | Phase::Applying(_) => {
                    return Err(LifecycleError::ActivationInProgress(
                        entry.target().expect("in-progress entry has a document"),
                    ));
                }
                Phase::Failed(_) | Phase::Native(_) => {
                    let target = self.allocate_document(entry.generation);
                    self.entries.insert(
                        label.to_owned(),
                        Entry {
                            generation: target.window,
                            phase: Phase::Preparing(target.document),
                        },
                    );
                    return Ok(ActivationDecision::Reserved(target));
                }
                Phase::Destroying => {
                    // A replacement may reuse the label while generation-specific
                    // teardown for the old native window is still completing.
                }
            }
        }

        let target = self.allocate_window_target();
        self.entries.insert(
            label.to_owned(),
            Entry {
                generation: target.window,
                phase: Phase::Preparing(target.document),
            },
        );
        Ok(ActivationDecision::Reserved(target))
    }

    pub(crate) fn invalidate_document(&mut self, label: &str) -> Option<FrontendTarget> {
        let entry = self.entries.get(label).copied()?;
        if matches!(entry.phase, Phase::Native(_) | Phase::Destroying) {
            return None;
        }

        let target = self.allocate_document(entry.generation);
        self.entries.insert(
            label.to_owned(),
            Entry {
                generation: target.window,
                phase: Phase::Preparing(target.document),
            },
        );
        Some(target)
    }

    pub(crate) fn prepare_document(&mut self, label: &str) -> Option<FrontendTarget> {
        let entry = self.entries.get(label).copied()?;

        match entry.phase {
            Phase::Preparing(_) | Phase::Applying(_) => entry.target(),
            Phase::Active(_) | Phase::Failed(_) => {
                let target = self.allocate_document(entry.generation);
                self.entries.insert(
                    label.to_owned(),
                    Entry {
                        generation: target.window,
                        phase: Phase::Preparing(target.document),
                    },
                );
                Some(target)
            }
            Phase::Native(_) | Phase::Destroying => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn preparing_target(&self, label: &str) -> Option<FrontendTarget> {
        self.entries.get(label).copied().and_then(|entry| {
            matches!(entry.phase, Phase::Preparing(_))
                .then(|| entry.target().expect("preparing entry has a document"))
        })
    }

    #[cfg(test)]
    pub(crate) fn current_target(&self, label: &str) -> Option<FrontendTarget> {
        self.entries.get(label).copied().and_then(Entry::target)
    }

    #[cfg(test)]
    pub(crate) fn active_target(&self, label: &str) -> Option<FrontendTarget> {
        self.entries.get(label).copied().and_then(|entry| {
            matches!(entry.phase, Phase::Active(_))
                .then(|| entry.target().expect("active entry has a document"))
        })
    }

    pub(crate) fn fail_preparation(&mut self, label: &str, target: FrontendTarget) -> bool {
        self.transition(
            label,
            target,
            |phase| matches!(phase, Phase::Preparing(_)),
            Phase::Failed(target.document),
        )
    }

    pub(crate) fn begin_native_apply(
        &mut self,
        label: &str,
        target: FrontendTarget,
    ) -> ReadinessDecision {
        let Some(entry) = self.entries.get(label).copied() else {
            return ReadinessDecision::Stale;
        };
        if !entry.matches(target) {
            return ReadinessDecision::Stale;
        }

        match entry.phase {
            Phase::Preparing(_) => {
                self.entries.insert(
                    label.to_owned(),
                    Entry {
                        generation: target.window,
                        phase: Phase::Applying(target.document),
                    },
                );
                ReadinessDecision::Apply
            }
            Phase::Applying(_) => ReadinessDecision::AlreadyApplying,
            Phase::Active(_) => ReadinessDecision::AlreadyActive,
            Phase::Failed(_) | Phase::Native(_) | Phase::Destroying => ReadinessDecision::Stale,
        }
    }

    pub(crate) fn commit_native_apply(&mut self, label: &str, target: FrontendTarget) -> bool {
        self.transition(
            label,
            target,
            |phase| matches!(phase, Phase::Applying(_)),
            Phase::Active(target.document),
        )
    }

    pub(crate) fn fail_native_apply(&mut self, label: &str, target: FrontendTarget) -> bool {
        self.transition(
            label,
            target,
            |phase| matches!(phase, Phase::Applying(_)),
            Phase::Failed(target.document),
        )
    }

    pub(crate) fn cancel_current(&mut self, label: &str) -> Option<FrontendTarget> {
        let entry = self.entries.get_mut(label)?;
        if entry.phase == Phase::Destroying {
            return None;
        }
        let target = entry.target()?;
        entry.phase = Phase::Native(target.document);
        Some(target)
    }

    fn transition(
        &mut self,
        label: &str,
        target: FrontendTarget,
        accepts: impl FnOnce(Phase) -> bool,
        next: Phase,
    ) -> bool {
        let Some(entry) = self.entries.get_mut(label) else {
            return false;
        };
        if !entry.matches(target) || !accepts(entry.phase) {
            return false;
        }
        entry.phase = next;
        true
    }

    pub(crate) fn generation(&self, label: &str) -> Option<Generation> {
        self.entries.get(label).map(|entry| entry.generation)
    }

    pub(crate) fn begin_destroy(&mut self, label: &str, generation: Generation) -> bool {
        let Some(entry) = self.entries.get_mut(label) else {
            return false;
        };
        if entry.generation != generation || entry.phase == Phase::Destroying {
            return false;
        }
        entry.phase = Phase::Destroying;
        true
    }

    pub(crate) fn begin_destroy_current(&mut self, label: &str) -> Option<Generation> {
        let generation = self.generation(label)?;
        self.begin_destroy(label, generation).then_some(generation)
    }

    pub(crate) fn finish_destroy(&mut self, label: &str, generation: Generation) -> bool {
        let should_remove = self.entries.get(label).is_some_and(|entry| {
            entry.generation == generation && entry.phase == Phase::Destroying
        });
        if should_remove {
            self.entries.remove(label);
        }
        should_remove
    }
}

#[derive(Debug)]
pub(crate) struct DecorationState {
    main_thread: ThreadId,
    lifecycle: Mutex<Lifecycle>,
    #[cfg(target_os = "macos")]
    traffic: Mutex<TrafficRegistry>,
}

impl DecorationState {
    pub(crate) fn new(main_thread: ThreadId) -> Self {
        Self {
            main_thread,
            lifecycle: Mutex::new(Lifecycle::default()),
            #[cfg(target_os = "macos")]
            traffic: Mutex::new(TrafficRegistry::default()),
        }
    }

    pub(crate) fn main_thread(&self) -> ThreadId {
        self.main_thread
    }

    fn lifecycle(&self) -> MutexGuard<'_, Lifecycle> {
        self.lifecycle.lock().unwrap()
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn with_traffic<T>(&self, action: impl FnOnce(&mut TrafficRegistry) -> T) -> T {
        let mut traffic = self.traffic.lock().unwrap();
        action(&mut traffic)
    }

    pub(crate) fn begin_activation(
        &self,
        label: &str,
    ) -> Result<ActivationDecision, LifecycleError> {
        self.lifecycle().begin_activation(label)
    }

    pub(crate) fn invalidate_document(&self, label: &str) -> Option<FrontendTarget> {
        self.lifecycle().invalidate_document(label)
    }

    pub(crate) fn prepare_document(&self, label: &str) -> Option<FrontendTarget> {
        self.lifecycle().prepare_document(label)
    }

    #[cfg(test)]
    pub(crate) fn preparing_target(&self, label: &str) -> Option<FrontendTarget> {
        self.lifecycle().preparing_target(label)
    }

    #[cfg(test)]
    pub(crate) fn current_target(&self, label: &str) -> Option<FrontendTarget> {
        self.lifecycle().current_target(label)
    }

    #[cfg(test)]
    pub(crate) fn active_target(&self, label: &str) -> Option<FrontendTarget> {
        self.lifecycle().active_target(label)
    }

    pub(crate) fn fail_preparation(&self, label: &str, target: FrontendTarget) -> bool {
        self.lifecycle().fail_preparation(label, target)
    }

    pub(crate) fn begin_native_apply(
        &self,
        label: &str,
        target: FrontendTarget,
    ) -> ReadinessDecision {
        self.lifecycle().begin_native_apply(label, target)
    }

    pub(crate) fn commit_native_apply(&self, label: &str, target: FrontendTarget) -> bool {
        self.lifecycle().commit_native_apply(label, target)
    }

    pub(crate) fn fail_native_apply(&self, label: &str, target: FrontendTarget) -> bool {
        self.lifecycle().fail_native_apply(label, target)
    }

    pub(crate) fn cancel_current(&self, label: &str) -> Option<FrontendTarget> {
        self.lifecycle().cancel_current(label)
    }

    #[cfg(test)]
    pub(crate) fn generation(&self, label: &str) -> Option<Generation> {
        self.lifecycle().generation(label)
    }

    #[cfg(test)]
    pub(crate) fn begin_destroy(&self, label: &str, generation: Generation) -> bool {
        self.lifecycle().begin_destroy(label, generation)
    }

    pub(crate) fn begin_destroy_current(&self, label: &str) -> Option<Generation> {
        self.lifecycle().begin_destroy_current(label)
    }

    pub(crate) fn finish_destroy(&self, label: &str, generation: Generation) -> bool {
        self.lifecycle().finish_destroy(label, generation)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ActivationDecision, DecorationState, DocumentToken, FrontendTarget, Generation,
        LifecycleError, ReadinessDecision,
    };
    use std::{
        panic::{catch_unwind, AssertUnwindSafe},
        thread,
    };

    fn reserve(state: &DecorationState, label: &str) -> FrontendTarget {
        match state.begin_activation(label).unwrap() {
            ActivationDecision::Reserved(target) => target,
            other => panic!("expected a reservation, got {other:?}"),
        }
    }

    fn activate(state: &DecorationState, label: &str) -> FrontendTarget {
        let target = reserve(state, label);
        assert_eq!(
            state.begin_native_apply(label, target),
            ReadinessDecision::Apply
        );
        assert!(state.commit_native_apply(label, target));
        target
    }

    #[test]
    fn initial_activation_prepares_a_window_generation_and_document_token() {
        let state = DecorationState::new(thread::current().id());
        let target = reserve(&state, "main");

        assert_ne!(target.window.get(), 0);
        assert_ne!(target.document.get(), 0);
        assert_eq!(state.preparing_target("main"), Some(target));
        assert_eq!(state.active_target("main"), None);
    }

    #[test]
    fn active_activation_is_idempotent() {
        let state = DecorationState::new(thread::current().id());
        let target = activate(&state, "main");

        assert_eq!(
            state.begin_activation("main"),
            Ok(ActivationDecision::AlreadyActive(target))
        );
    }

    #[test]
    fn concurrent_activation_is_rejected_explicitly() {
        let state = DecorationState::new(thread::current().id());
        let target = reserve(&state, "main");

        assert_eq!(
            state.begin_activation("main"),
            Err(LifecycleError::ActivationInProgress(target))
        );
    }

    #[test]
    fn matching_readiness_can_apply_and_commit() {
        let state = DecorationState::new(thread::current().id());
        let target = reserve(&state, "main");

        assert_eq!(
            state.begin_native_apply("main", target),
            ReadinessDecision::Apply
        );
        assert!(state.commit_native_apply("main", target));
        assert_eq!(state.active_target("main"), Some(target));
    }

    #[test]
    fn navigation_invalidates_the_previous_document_before_reconciliation() {
        let state = DecorationState::new(thread::current().id());
        let old = activate(&state, "main");

        let replacement = state.invalidate_document("main").unwrap();
        assert_eq!(replacement.window, old.window);
        assert_ne!(replacement.document, old.document);
        assert_eq!(state.active_target("main"), None);
        assert_eq!(
            state.begin_native_apply("main", old),
            ReadinessDecision::Stale
        );
        assert_eq!(state.preparing_target("main"), Some(replacement));
    }

    #[test]
    fn finished_reconciliation_reuses_an_existing_preparation() {
        let state = DecorationState::new(thread::current().id());
        let target = reserve(&state, "main");
        assert_eq!(state.prepare_document("main"), Some(target));
    }

    #[test]
    fn failed_frontend_preparation_is_safe_and_retryable() {
        let state = DecorationState::new(thread::current().id());
        let failed = reserve(&state, "main");

        assert!(state.fail_preparation("main", failed));
        assert_eq!(state.active_target("main"), None);
        assert_eq!(state.preparing_target("main"), None);

        let retry = state.prepare_document("main").unwrap();
        assert_eq!(retry.window, failed.window);
        assert_ne!(retry.document, failed.document);
        assert_eq!(state.preparing_target("main"), Some(retry));
    }

    #[test]
    fn failed_native_apply_cannot_be_committed_by_a_late_success() {
        let state = DecorationState::new(thread::current().id());
        let failed = reserve(&state, "main");
        assert_eq!(
            state.begin_native_apply("main", failed),
            ReadinessDecision::Apply
        );

        assert!(state.fail_native_apply("main", failed));
        assert!(!state.commit_native_apply("main", failed));
        assert_eq!(state.active_target("main"), None);
    }

    #[test]
    fn cancelling_a_preparation_invalidates_late_readiness_and_preserves_other_windows() {
        let state = DecorationState::new(thread::current().id());
        let cancelled = reserve(&state, "main");
        let other = activate(&state, "secondary");

        assert_eq!(state.cancel_current("main"), Some(cancelled));
        assert_eq!(
            state.begin_native_apply("main", cancelled),
            ReadinessDecision::Stale
        );
        assert_eq!(state.active_target("secondary"), Some(other));

        let retry = reserve(&state, "main");
        assert_eq!(retry.window, cancelled.window);
        assert_ne!(retry.document, cancelled.document);
    }

    #[test]
    fn explicit_native_restore_stays_off_across_navigation_until_reactivated() {
        let state = DecorationState::new(thread::current().id());
        let cancelled = activate(&state, "main");

        assert_eq!(state.cancel_current("main"), Some(cancelled));
        assert_eq!(state.cancel_current("main"), Some(cancelled));
        assert_eq!(state.invalidate_document("main"), None);
        assert_eq!(state.prepare_document("main"), None);
        assert_eq!(
            state.begin_native_apply("main", cancelled),
            ReadinessDecision::Stale
        );

        let reactivated = reserve(&state, "main");
        assert_eq!(reactivated.window, cancelled.window);
        assert_ne!(reactivated.document, cancelled.document);
    }

    #[test]
    fn duplicate_readiness_for_an_active_document_is_idempotent() {
        let state = DecorationState::new(thread::current().id());
        let target = activate(&state, "main");

        assert_eq!(
            state.begin_native_apply("main", target),
            ReadinessDecision::AlreadyActive
        );
    }

    #[test]
    fn duplicate_readiness_during_native_apply_is_explicit() {
        let state = DecorationState::new(thread::current().id());
        let target = reserve(&state, "main");
        assert_eq!(
            state.begin_native_apply("main", target),
            ReadinessDecision::Apply
        );
        assert_eq!(
            state.begin_native_apply("main", target),
            ReadinessDecision::AlreadyApplying
        );
    }

    #[test]
    fn stale_window_destruction_cannot_remove_a_replacement() {
        let state = DecorationState::new(thread::current().id());
        let old = reserve(&state, "main");
        assert!(state.begin_destroy("main", old.window));

        let replacement = reserve(&state, "main");
        assert_ne!(replacement.window, old.window);
        assert!(!state.finish_destroy("main", old.window));
        assert_eq!(state.preparing_target("main"), Some(replacement));
    }

    #[test]
    fn destroy_current_invalidates_frontend_dispatch() {
        let state = DecorationState::new(thread::current().id());
        let target = activate(&state, "main");

        assert_eq!(state.begin_destroy_current("main"), Some(target.window));
        assert_eq!(state.current_target("main"), None);
        assert!(state.finish_destroy("main", target.window));
        assert_eq!(state.generation("main"), None);
    }

    #[test]
    fn token_types_are_not_interchangeable() {
        let target = FrontendTarget {
            window: Generation::from_raw(7),
            document: DocumentToken::from_raw(11),
        };
        assert_eq!(target.window.get(), 7);
        assert_eq!(target.document.get(), 11);
    }

    #[test]
    fn frontend_targets_accept_only_nonzero_token_values() {
        assert_eq!(FrontendTarget::from_values(0, 1), None);
        assert_eq!(FrontendTarget::from_values(1, 0), None);
        assert_eq!(
            FrontendTarget::from_values(7, 11),
            Some(FrontendTarget {
                window: Generation::from_raw(7),
                document: DocumentToken::from_raw(11),
            })
        );
    }

    #[test]
    fn poisoned_state_is_not_recovered() {
        let state = DecorationState::new(thread::current().id());
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _guard = state.lifecycle.lock().unwrap();
            panic!("poison the lifecycle mutex");
        }));

        assert!(catch_unwind(AssertUnwindSafe(|| state.begin_activation("main"))).is_err());
    }

    #[test]
    fn managed_state_records_the_event_loop_thread() {
        let current = thread::current().id();
        let state = DecorationState::new(current);
        assert_eq!(state.main_thread(), current);
    }
}
