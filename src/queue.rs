use crate::providers::Track;

#[derive(Debug, Default)]
pub struct Queue {
    pub tracks: Vec<Track>,
    pub current: Option<usize>,
}

#[derive(Debug)]
pub enum Removed {
    /// La pista eliminada no era la que sonaba.
    Other,
    /// Era la que sonaba; en su lugar (mismo índice) quedó otra pista.
    CurrentWithSuccessor(usize),
    /// Era la que sonaba y era la última; no hay sucesora.
    CurrentWithoutSuccessor,
}

impl Queue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.current.and_then(|index| self.tracks.get(index))
    }

    pub fn contains(&self, track: &Track) -> Option<usize> {
        self.tracks
            .iter()
            .position(|t| t.provider == track.provider && t.id == track.id)
    }

    /// Añade si no está; devuelve el índice de la pista en la cola.
    pub fn add(&mut self, track: Track) -> usize {
        if let Some(index) = self.contains(&track) {
            return index;
        }
        self.tracks.push(track);
        self.tracks.len() - 1
    }

    pub fn remove(&mut self, index: usize) -> Option<Removed> {
        if index >= self.tracks.len() {
            return None;
        }
        self.tracks.remove(index);
        let removed = match self.current {
            Some(current) if current == index => {
                if index < self.tracks.len() {
                    Removed::CurrentWithSuccessor(index)
                } else {
                    self.current = None;
                    Removed::CurrentWithoutSuccessor
                }
            }
            Some(current) if current > index => {
                self.current = Some(current - 1);
                Removed::Other
            }
            _ => Removed::Other,
        };
        Some(removed)
    }

    pub fn move_up(&mut self, index: usize) -> bool {
        if index == 0 || index >= self.tracks.len() {
            return false;
        }
        self.tracks.swap(index, index - 1);
        self.current = self.current.map(|current| {
            if current == index {
                index - 1
            } else if current == index - 1 {
                index
            } else {
                current
            }
        });
        true
    }

    pub fn move_down(&mut self, index: usize) -> bool {
        if index + 1 >= self.tracks.len() {
            return false;
        }
        self.move_up(index + 1)
    }

    /// Baraja la cola en el sitio; la pista que suena mantiene su posición.
    pub fn shuffle(&mut self) {
        let Some(current) = self.current else {
            crate::random::shuffle(&mut self.tracks);
            return;
        };
        if current >= self.tracks.len() {
            return;
        }
        let playing = self.tracks[current].clone();
        crate::random::shuffle(&mut self.tracks);
        if let Some(pos) = self
            .tracks
            .iter()
            .position(|t| t.provider == playing.provider && t.id == playing.id)
        {
            self.tracks.swap(pos, current);
        }
    }

    /// Índice de la siguiente pista; None si no hay actual o es la última.
    pub fn next_index(&self) -> Option<usize> {
        match self.current {
            Some(current) if current + 1 < self.tracks.len() => Some(current + 1),
            None if !self.tracks.is_empty() => Some(0),
            _ => None,
        }
    }

    pub fn prev_index(&self) -> Option<usize> {
        match self.current {
            Some(current) if current > 0 => Some(current - 1),
            None if !self.tracks.is_empty() => Some(0),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ProviderKind;

    fn track(id: &str) -> Track {
        Track {
            id: id.to_string(),
            title: format!("título {id}"),
            artist: "artista".to_string(),
            duration: None,
            source_url: format!("https://example.com/{id}"),
            provider: ProviderKind::Youtube,
        }
    }

    fn queue3() -> Queue {
        let mut queue = Queue::new();
        queue.add(track("a"));
        queue.add(track("b"));
        queue.add(track("c"));
        queue
    }

    #[test]
    fn add_dedupes_and_returns_index() {
        let mut queue = queue3();
        assert_eq!(queue.add(track("d")), 3);
        assert_eq!(queue.add(track("b")), 1);
        assert_eq!(queue.len(), 4);
    }

    #[test]
    fn remove_other_adjusts_current() {
        let mut queue = queue3();
        queue.current = Some(2);
        assert!(matches!(queue.remove(0), Some(Removed::Other)));
        assert_eq!(queue.current, Some(1));
        assert_eq!(queue.current_track().unwrap().id, "c");
    }

    #[test]
    fn remove_current_keeps_successor_at_same_index() {
        let mut queue = queue3();
        queue.current = Some(1);
        match queue.remove(1) {
            Some(Removed::CurrentWithSuccessor(index)) => assert_eq!(index, 1),
            other => panic!("esperaba sucesora: {other:?}"),
        }
        assert_eq!(queue.tracks[1].id, "c");
        // current lo decide la app al reproducir la sucesora; la cola no lo toca
        assert_eq!(queue.current, Some(1));
    }

    #[test]
    fn remove_current_last_clears_current() {
        let mut queue = queue3();
        queue.current = Some(2);
        assert!(matches!(
            queue.remove(2),
            Some(Removed::CurrentWithoutSuccessor)
        ));
        assert_eq!(queue.current, None);
        assert_eq!(queue.len(), 2);
    }

    #[test]
    fn move_up_down_with_bounds() {
        let mut queue = queue3();
        assert!(!queue.move_up(0));
        assert!(queue.move_up(1));
        assert_eq!(queue.tracks[0].id, "b");
        assert!(queue.move_down(0));
        assert_eq!(queue.tracks[0].id, "a");
        assert!(!queue.move_down(2));
    }

    #[test]
    fn move_tracks_current_marker() {
        let mut queue = queue3();
        queue.current = Some(1); // suena "b"
        queue.move_up(1); // "b" sube: [b, a, c]
        assert_eq!(queue.current, Some(0));
        queue.move_down(1); // mueve "a" hacia abajo: [b, c, a]; "b" no se toca
        assert_eq!(queue.current, Some(0));
        assert_eq!(queue.tracks[1].id, "c");
    }

    #[test]
    fn next_prev_without_loop() {
        let mut queue = queue3();
        assert_eq!(queue.next_index(), Some(0));
        assert_eq!(queue.prev_index(), Some(0));
        queue.current = Some(0);
        assert_eq!(queue.next_index(), Some(1));
        assert_eq!(queue.prev_index(), None);
        queue.current = Some(2);
        assert_eq!(queue.next_index(), None);
        assert_eq!(queue.prev_index(), Some(1));
        let empty = Queue::new();
        assert_eq!(empty.next_index(), None);
    }

    #[test]
    fn shuffle_keeps_membership_and_current_track() {
        let mut queue = queue3();
        queue.current = Some(1); // suena "b"
        queue.shuffle();
        let mut ids: Vec<&str> = queue.tracks.iter().map(|t| t.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, ["a", "b", "c"]);
        assert_eq!(queue.current, Some(1));
        assert_eq!(queue.current_track().unwrap().id, "b");
    }

    #[test]
    fn shuffle_without_current_just_shuffles() {
        let mut queue = queue3();
        queue.shuffle();
        assert_eq!(queue.current, None);
        let mut ids: Vec<&str> = queue.tracks.iter().map(|t| t.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, ["a", "b", "c"]);
    }
}
