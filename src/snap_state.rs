use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SnapEvent {
    MouseEnter,
    MouseLeave,
    MouseDown,
    MouseUp,
    Click,
    FullscreenEnter,
    FullscreenExit,
}

impl SnapEvent {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::MouseEnter => "snap-mouseenter",
            Self::MouseLeave => "snap-mouseleave",
            Self::MouseDown => "snap-mousedown",
            Self::MouseUp => "snap-mouseup",
            Self::Click => "snap-click",
            Self::FullscreenEnter => "fullscreen-did-enter",
            Self::FullscreenExit => "fullscreen-did-exit",
        }
    }
}

type EventCallback = Arc<dyn Fn(SnapEvent) + Send + Sync + 'static>;
type MoveCallback = Arc<dyn Fn(i32, i32) + Send + Sync + 'static>;

#[derive(Clone)]
pub(crate) struct Callbacks {
    event: EventCallback,
    move_event: MoveCallback,
}

impl Callbacks {
    pub(crate) fn new(event: EventCallback, move_event: MoveCallback) -> Self {
        Self { event, move_event }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Geometry {
    titlebar_height: u32,
    button_width: u32,
    right_index: u32,
}

impl Geometry {
    pub(crate) fn new(titlebar_height: u32, button_width: u32, right_index: u32) -> Self {
        Self {
            titlebar_height,
            button_width,
            right_index,
        }
    }
}

struct Entry {
    overlay: isize,
    geometry: Geometry,
    hovering: bool,
    pressing: bool,
    fullscreen: bool,
    last_position: Option<(i32, i32)>,
    callbacks: Callbacks,
}

pub(crate) struct Removed {
    parent: isize,
    entry: Entry,
}

impl Removed {
    pub(crate) fn parent(&self) -> isize {
        self.parent
    }

    pub(crate) fn overlay(&self) -> isize {
        self.entry.overlay
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Position {
    overlay: isize,
    geometry: Geometry,
    fullscreen: bool,
}

impl Position {
    pub(crate) fn overlay(self) -> isize {
        self.overlay
    }

    pub(crate) fn titlebar_height(self) -> u32 {
        self.geometry.titlebar_height
    }

    pub(crate) fn button_width(self) -> u32 {
        self.geometry.button_width
    }

    pub(crate) fn right_index(self) -> u32 {
        self.geometry.right_index
    }

    pub(crate) fn fullscreen(self) -> bool {
        self.fullscreen
    }
}

enum Effect {
    Event(EventCallback, SnapEvent),
    Move(MoveCallback, i32, i32),
}

#[derive(Default)]
pub(crate) struct Effects(Vec<Effect>);

impl Effects {
    fn event(&mut self, callbacks: &Callbacks, event: SnapEvent) {
        self.0.push(Effect::Event(callbacks.event.clone(), event));
    }

    fn move_event(&mut self, callbacks: &Callbacks, x: i32, y: i32) {
        self.0
            .push(Effect::Move(callbacks.move_event.clone(), x, y));
    }

    pub(crate) fn entered(&self) -> bool {
        self.0
            .iter()
            .any(|effect| matches!(effect, Effect::Event(_, SnapEvent::MouseEnter)))
    }

    pub(crate) fn dispatch(self) {
        for effect in self.0 {
            match effect {
                Effect::Event(callback, event) => callback(event),
                Effect::Move(callback, x, y) => callback(x, y),
            }
        }
    }
}

#[derive(Default)]
pub(crate) struct Registry {
    entries: HashMap<isize, Entry>,
}

impl Registry {
    pub(crate) fn insert(
        &mut self,
        parent: isize,
        overlay: isize,
        geometry: Geometry,
        callbacks: Callbacks,
    ) -> Result<(), &'static str> {
        if self.entries.contains_key(&parent) {
            return Err("a snap overlay is already installed for this window");
        }
        self.entries.insert(
            parent,
            Entry {
                overlay,
                geometry,
                hovering: false,
                pressing: false,
                fullscreen: false,
                last_position: None,
                callbacks,
            },
        );
        Ok(())
    }

    pub(crate) fn remove_parent(&mut self, parent: isize) -> Option<Removed> {
        self.entries
            .remove(&parent)
            .map(|entry| Removed { parent, entry })
    }

    pub(crate) fn remove_overlay(&mut self, overlay: isize) -> Option<Removed> {
        let parent = self.parent_for_overlay(overlay)?;
        self.remove_parent(parent)
    }

    pub(crate) fn position(&self, parent: isize) -> Option<Position> {
        self.entries.get(&parent).map(|entry| Position {
            overlay: entry.overlay,
            geometry: entry.geometry,
            fullscreen: entry.fullscreen,
        })
    }

    pub(crate) fn set_fullscreen(
        &mut self,
        parent: isize,
        fullscreen: bool,
    ) -> Option<(Position, Effects)> {
        let entry = self.entries.get_mut(&parent)?;
        if entry.fullscreen == fullscreen {
            return None;
        }
        entry.fullscreen = fullscreen;
        let mut effects = Effects::default();
        effects.event(
            &entry.callbacks,
            if fullscreen {
                SnapEvent::FullscreenEnter
            } else {
                SnapEvent::FullscreenExit
            },
        );
        Some((
            Position {
                overlay: entry.overlay,
                geometry: entry.geometry,
                fullscreen,
            },
            effects,
        ))
    }

    pub(crate) fn rollback_fullscreen(&mut self, parent: isize, attempted: bool) -> bool {
        let Some(entry) = self.entries.get_mut(&parent) else {
            return false;
        };
        if entry.fullscreen != attempted {
            return false;
        }
        entry.fullscreen = !attempted;
        true
    }

    pub(crate) fn parent_for_overlay(&self, overlay: isize) -> Option<isize> {
        self.entries
            .iter()
            .find_map(|(parent, entry)| (entry.overlay == overlay).then_some(*parent))
    }

    pub(crate) fn mouse_move(&mut self, overlay: isize, x: i32, y: i32) -> Effects {
        let Some(parent) = self.parent_for_overlay(overlay) else {
            return Effects::default();
        };
        let entry = self
            .entries
            .get_mut(&parent)
            .expect("overlay lookup returned an existing parent");
        let mut effects = Effects::default();
        if entry.last_position != Some((x, y)) {
            entry.last_position = Some((x, y));
            effects.move_event(&entry.callbacks, x, y);
        }
        if !entry.hovering {
            entry.hovering = true;
            effects.event(&entry.callbacks, SnapEvent::MouseEnter);
        }
        effects
    }

    pub(crate) fn mouse_leave(&mut self, overlay: isize) -> Effects {
        let Some(parent) = self.parent_for_overlay(overlay) else {
            return Effects::default();
        };
        let entry = self
            .entries
            .get_mut(&parent)
            .expect("overlay lookup returned an existing parent");
        let should_emit = entry.hovering || entry.pressing;
        entry.hovering = false;
        entry.pressing = false;
        let mut effects = Effects::default();
        if should_emit {
            effects.event(&entry.callbacks, SnapEvent::MouseLeave);
        }
        effects
    }

    pub(crate) fn mouse_down(&mut self, overlay: isize) -> Effects {
        let Some(parent) = self.parent_for_overlay(overlay) else {
            return Effects::default();
        };
        let entry = self
            .entries
            .get_mut(&parent)
            .expect("overlay lookup returned an existing parent");
        entry.pressing = true;
        let mut effects = Effects::default();
        effects.event(&entry.callbacks, SnapEvent::MouseDown);
        effects
    }

    pub(crate) fn mouse_up(&mut self, overlay: isize) -> Effects {
        let Some(parent) = self.parent_for_overlay(overlay) else {
            return Effects::default();
        };
        let entry = self
            .entries
            .get_mut(&parent)
            .expect("overlay lookup returned an existing parent");
        let click = entry.pressing;
        entry.pressing = false;
        let mut effects = Effects::default();
        effects.event(&entry.callbacks, SnapEvent::MouseUp);
        if click {
            effects.event(&entry.callbacks, SnapEvent::Click);
        }
        effects
    }
}

#[cfg(test)]
mod tests {
    use super::{Callbacks, Geometry, Registry, SnapEvent};
    use std::{
        panic::{catch_unwind, AssertUnwindSafe},
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Mutex, Weak,
        },
    };

    struct RegistryLockProbe {
        registry: Weak<Mutex<Registry>>,
        dropped_outside_lock: Arc<AtomicBool>,
    }

    impl Drop for RegistryLockProbe {
        fn drop(&mut self) {
            let lock_available = self
                .registry
                .upgrade()
                .is_some_and(|registry| registry.try_lock().is_ok());
            self.dropped_outside_lock
                .store(lock_available, Ordering::SeqCst);
        }
    }

    fn callbacks(events: Arc<Mutex<Vec<String>>>) -> Callbacks {
        let event_events = events.clone();
        Callbacks::new(
            Arc::new(move |event| {
                event_events.lock().unwrap().push(format!("{event:?}"));
            }),
            Arc::new(move |x, y| {
                events.lock().unwrap().push(format!("Move({x}, {y})"));
            }),
        )
    }

    #[test]
    fn duplicate_install_is_rejected_without_replacing_the_original() {
        let mut registry = Registry::default();
        let events = Arc::new(Mutex::new(Vec::new()));
        registry
            .insert(10, 100, Geometry::new(32, 58, 1), callbacks(events.clone()))
            .unwrap();

        assert!(registry
            .insert(10, 101, Geometry::new(40, 60, 0), callbacks(events))
            .is_err());

        assert_eq!(registry.position(10).unwrap().overlay(), 100);
    }

    #[test]
    fn pointer_transitions_are_ordered_and_duplicate_moves_are_suppressed() {
        let mut registry = Registry::default();
        let events = Arc::new(Mutex::new(Vec::new()));
        registry
            .insert(10, 100, Geometry::new(32, 58, 1), callbacks(events.clone()))
            .unwrap();

        let first_move = registry.mouse_move(100, 4, 5);
        assert!(first_move.entered());
        first_move.dispatch();
        registry.mouse_move(100, 4, 5).dispatch();
        registry.mouse_down(100).dispatch();
        registry.mouse_up(100).dispatch();
        registry.mouse_leave(100).dispatch();

        assert_eq!(
            *events.lock().unwrap(),
            [
                "Move(4, 5)",
                "MouseEnter",
                "MouseDown",
                "MouseUp",
                "Click",
                "MouseLeave",
            ]
        );
    }

    #[test]
    fn fullscreen_changes_are_deduped_and_expose_native_visibility_state() {
        let mut registry = Registry::default();
        let events = Arc::new(Mutex::new(Vec::new()));
        registry
            .insert(10, 100, Geometry::new(32, 58, 1), callbacks(events.clone()))
            .unwrap();

        let (entered, effects) = registry
            .set_fullscreen(10, true)
            .expect("first fullscreen transition");
        assert!(entered.fullscreen());
        assert_eq!(entered.overlay(), 100);
        effects.dispatch();
        assert!(registry.set_fullscreen(10, true).is_none());

        let (exited, effects) = registry.set_fullscreen(10, false).expect("fullscreen exit");
        assert!(!exited.fullscreen());
        effects.dispatch();

        assert_eq!(
            *events.lock().unwrap(),
            ["FullscreenEnter", "FullscreenExit"]
        );
    }

    #[test]
    fn failed_fullscreen_transition_can_be_rolled_back_and_retried() {
        let mut registry = Registry::default();
        let events = Arc::new(Mutex::new(Vec::new()));
        registry
            .insert(10, 100, Geometry::new(32, 58, 1), callbacks(events))
            .unwrap();
        let _enter = registry.set_fullscreen(10, true).expect("fullscreen entry");

        let _failed_exit = registry
            .set_fullscreen(10, false)
            .expect("first fullscreen exit");
        assert!(registry.rollback_fullscreen(10, false));
        assert!(registry.position(10).unwrap().fullscreen());
        assert!(registry.set_fullscreen(10, false).is_some());
    }

    #[test]
    fn effects_can_reenter_the_registry_because_callbacks_are_not_run_under_its_lock() {
        let registry = Arc::new(Mutex::new(Registry::default()));
        let callback_registry = registry.clone();
        let event = Arc::new(move |event| {
            assert_eq!(event, SnapEvent::MouseEnter);
            assert!(callback_registry.try_lock().is_ok());
        });
        registry
            .lock()
            .unwrap()
            .insert(
                10,
                100,
                Geometry::new(32, 58, 1),
                Callbacks::new(event, Arc::new(|_, _| {})),
            )
            .unwrap();

        let effects = registry.lock().unwrap().mouse_move(100, 0, 0);
        effects.dispatch();
    }

    #[test]
    fn removed_callbacks_are_dropped_after_the_registry_lock_is_released() {
        let registry = Arc::new(Mutex::new(Registry::default()));
        let dropped_outside_lock = Arc::new(AtomicBool::new(false));
        let probe = RegistryLockProbe {
            registry: Arc::downgrade(&registry),
            dropped_outside_lock: dropped_outside_lock.clone(),
        };
        let callback = Arc::new(move |_| {
            let _keep_probe_alive = &probe;
        });
        registry
            .lock()
            .unwrap()
            .insert(
                10,
                100,
                Geometry::new(32, 58, 1),
                Callbacks::new(callback, Arc::new(|_, _| {})),
            )
            .unwrap();

        let removed = registry
            .lock()
            .unwrap()
            .remove_parent(10)
            .expect("registered overlay");
        drop(removed);

        assert!(dropped_outside_lock.load(Ordering::SeqCst));
    }

    #[test]
    fn callback_panics_do_not_poison_the_registry_lock() {
        let registry = Arc::new(Mutex::new(Registry::default()));
        registry
            .lock()
            .unwrap()
            .insert(
                10,
                100,
                Geometry::new(32, 58, 1),
                Callbacks::new(Arc::new(|_| panic!("callback panic")), Arc::new(|_, _| {})),
            )
            .unwrap();

        let effects = registry.lock().unwrap().mouse_move(100, 0, 0);
        assert!(catch_unwind(AssertUnwindSafe(|| effects.dispatch())).is_err());
        assert!(registry.lock().is_ok());
    }

    #[test]
    fn removing_an_unexpected_overlay_invalidates_its_parent_entry() {
        let mut registry = Registry::default();
        let events = Arc::new(Mutex::new(Vec::new()));
        registry
            .insert(10, 100, Geometry::new(32, 58, 1), callbacks(events))
            .unwrap();

        let removed = registry.remove_overlay(100).expect("registered overlay");

        assert_eq!(removed.parent(), 10);
        assert_eq!(removed.overlay(), 100);
        assert!(registry.position(10).is_none());
    }

    #[test]
    fn position_is_an_owned_snapshot_of_the_geometry() {
        let mut registry = Registry::default();
        let events = Arc::new(Mutex::new(Vec::new()));
        registry
            .insert(10, 100, Geometry::new(32, 58, 2), callbacks(events))
            .unwrap();

        let position = registry.position(10).unwrap();
        let _removed = registry.remove_parent(10).unwrap();

        assert_eq!(position.overlay(), 100);
        assert_eq!(position.titlebar_height(), 32);
        assert_eq!(position.button_width(), 58);
        assert_eq!(position.right_index(), 2);
    }

    #[test]
    fn native_event_names_are_closed_over_the_frontend_dispatcher_contract() {
        assert_eq!(SnapEvent::MouseEnter.as_str(), "snap-mouseenter");
        assert_eq!(SnapEvent::MouseLeave.as_str(), "snap-mouseleave");
        assert_eq!(SnapEvent::MouseDown.as_str(), "snap-mousedown");
        assert_eq!(SnapEvent::MouseUp.as_str(), "snap-mouseup");
        assert_eq!(SnapEvent::Click.as_str(), "snap-click");
    }
}
