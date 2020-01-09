// Allows root origin to change the maximum number of members in society.
// Max membership count must be greater than 1.
//
// The dispatch origin for this call must be from _ROOT_.
//
// Parameters:
// - `max` - The maximum number of members for the society.
//
// # <weight>
// - One storage write to update the max. O(1)
// - One event.
//
// Total Complexity: O(1)
// # </weight>

// #[weight = SimpleDispatchInfo::FixedNormal(10_000)]
// fn set_max_members(origin, max: u32) {
//     ensure_root(origin)?;
//     ensure!(max > 1, Error::<T, I>::MaxMembers);
//     MaxMembers::<I>::put(max);
//     Self::deposit_event(RawEvent::NewMaxMembers(max));
// }

// Each of these are objects that implement the trait which means that they are consumed by the meta origin
// - group size
// - spending rate
// - new member (join) rate
// - kicking rate (include society)

#[non-exhaustive]
pub enum Version {
    /// TODO: could make this a wrapper around a semver type?
    /// - look into ontology rlay project
    V1,
}