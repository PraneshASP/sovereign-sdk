use borsh::{BorshDeserialize, BorshSerialize};
use serde::de::DeserializeOwned;
use serde::Serialize;
use sov_rollup_interface::maybestd::sync::atomic::AtomicUsize;
use sov_rollup_interface::maybestd::sync::Mutex;
use sov_rollup_interface::maybestd::vec::Vec;

/// A witness is a value produced during native execution that is then used by
/// the zkVM circuit to produce proofs.
///
/// Witnesses are typically used to abstract away storage access from inside the
/// zkVM. For every read operation performed by the native code, a hint can be
/// added and the zkVM circuit can then read the same hint. Hints are replayed
/// to [`Witness::get_hint`] in the same order
/// they were added via [`Witness::add_hint`].
// TODO: Refactor witness trait so it only require Serialize / Deserialize
//   https://github.com/Sovereign-Labs/sovereign-sdk/issues/263
pub trait Witness: Default + Serialize + DeserializeOwned {
    /// Adds a serializable "hint" to the witness value, which can be later
    /// read by the zkVM circuit.
    ///
    /// This method **SHOULD** only be called from the native execution
    /// environment.
    fn add_hint<T: BorshSerialize>(&self, hint: T);

    /// Retrieves a "hint" from the witness value.
    fn get_hint<T: BorshDeserialize>(&self) -> T;

    /// Adds all hints from `rhs` to `self`.
    fn merge(&self, rhs: &Self);
}

/// A [`Vec`]-based implementation of [`Witness`] with no special logic.
///
/// # Example
///
/// ```
/// use sov_state::{ArrayWitness, Witness};
///
/// let witness = ArrayWitness::default();
///
/// witness.add_hint(1u64);
/// witness.add_hint(2u64);
///
/// assert_eq!(witness.get_hint::<u64>(), 1u64);
/// assert_eq!(witness.get_hint::<u64>(), 2u64);
/// ```
#[derive(Default, Debug)]
#[cfg_attr(feature = "std", derive(Serialize, serde::Deserialize))]
#[cfg_attr(not(feature = "std"), allow(dead_code))]
pub struct ArrayWitness {
    next_idx: AtomicUsize,
    hints: Mutex<Vec<Vec<u8>>>,
}

#[cfg(feature = "std")]
impl Witness for ArrayWitness {
    fn add_hint<T: BorshSerialize>(&self, hint: T) {
        self.hints.lock().unwrap().push(hint.try_to_vec().unwrap())
    }

    fn get_hint<T: BorshDeserialize>(&self) -> T {
        use sov_rollup_interface::maybestd::io;

        let idx = self
            .next_idx
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let hints_lock = self.hints.lock().unwrap();
        T::deserialize_reader(&mut io::Cursor::new(&hints_lock[idx]))
            .expect("Hint deserialization should never fail")
    }

    fn merge(&self, rhs: &Self) {
        let rhs_next_idx = rhs.next_idx.load(std::sync::atomic::Ordering::SeqCst);
        let mut lhs_hints_lock = self.hints.lock().unwrap();
        let mut rhs_hints_lock = rhs.hints.lock().unwrap();
        lhs_hints_lock.extend(rhs_hints_lock.drain(rhs_next_idx..))
    }
}
