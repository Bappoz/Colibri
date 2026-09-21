//! Runs systems, in order, once per frame.
//!
//! A system is any `FnMut(&mut World, f64)`: it gets the whole world and the
//! frame's `dt`. Execution is sequential and follows the order of
//! [`Schedule::add`]. Systems that touch disjoint components could run in
//! parallel one day; that needs access declarations and is deliberately not
//! attempted yet.

use crate::ecs::World;

/// A boxed system, so closures with different captures share one `Vec`.
type System = Box<dyn FnMut(&mut World, f64)>;

/// An ordered list of systems.
#[derive(Default)]
pub struct Schedule {
    systems: Vec<System>,
}

impl Schedule {
    /// An empty Schedule
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a system: it runs after every system added before it
    pub fn add(&mut self, system: impl FnMut(&mut World, f64) + 'static) -> &mut Self {
        self.systems.push(Box::new(system));
        self
    }

    /// Runs every system once, in insertion order, with the same `dt`
    pub fn run(&mut self, world: &mut World, dt: f64) {
        for system in &mut self.systems {
            system(world, dt);
        }
    }

    /// How many sysmte are scheduled
    pub fn len(&self) -> usize {
        self.systems.len()
    }

    /// Whether nothing is scheduled
    pub fn is_empty(&self) -> bool {
        self.systems.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// A ordem de `add` é a ordem de execução.
    #[test]
    fn systems_run_in_the_order_they_were_added() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut schedule = Schedule::new();
        let (first, second) = (Rc::clone(&log), Rc::clone(&log));
        schedule
            .add(move |_, _| first.borrow_mut().push("first"))
            .add(move |_, _| second.borrow_mut().push("second"));

        schedule.run(&mut World::new(), 0.0);

        assert_eq!(*log.borrow(), ["first", "second"]);
    }

    /// Sistema recebe o `dt` e mantém estado próprio entre frames (`FnMut`).
    #[test]
    fn systems_receive_dt_and_keep_state_between_runs() {
        let mut ticks = 0;
        let total = Rc::new(RefCell::new(0.0));
        let seen = Rc::clone(&total);
        let mut schedule = Schedule::new();
        schedule.add(move |_, dt| {
            ticks += 1;
            *seen.borrow_mut() += dt * f64::from(ticks);
        });

        let mut world = World::new();
        schedule.run(&mut world, 0.5);
        schedule.run(&mut world, 0.5);

        // 0.5*1 + 0.5*2
        assert_eq!(*total.borrow(), 1.5);
    }
}
