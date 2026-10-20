//! Azar sin dependencias para el modo radio (selección de artistas,
//! palabras clave y barajado de resultados).

/// Índice aleatorio en [0, n). Sesgo de módulo irrelevante para este uso.
pub(crate) fn below(n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    (crate::state_file::random_suffix() % n as u64) as usize
}

/// Fisher-Yates con la semilla del SO (RandomState por llamada).
pub(crate) fn shuffle<T>(items: &mut [T]) {
    for i in (1..items.len()).rev() {
        let j = below(i + 1);
        items.swap(i, j);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn below_stays_in_bounds() {
        for _ in 0..200 {
            assert!(below(7) < 7);
        }
        assert_eq!(below(0), 0);
        assert_eq!(below(1), 0);
    }

    #[test]
    fn shuffle_preserves_membership() {
        let mut items: Vec<u32> = (0..20).collect();
        shuffle(&mut items);
        items.sort();
        assert_eq!(items, (0..20).collect::<Vec<_>>());
    }
}
