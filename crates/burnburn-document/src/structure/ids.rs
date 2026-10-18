use std::fmt;

use burnburn_types::StableId;

macro_rules! document_id {
    ($(#[$doc:meta])* $name:ident, $prefix:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u64);

        impl StableId for $name {
            fn from_raw(raw: u64) -> Self {
                $name(raw)
            }
            fn raw(self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!($prefix, "{}"), self.0)
            }
        }
    };
}

document_id!(
    /// A non-destructive modifier chain; also `DerivedOwner::Modifier`.
    ModifierId,
    "mod"
);
document_id!(
    /// A named associative reference to an object, edge or vertex.
    ReferenceId,
    "ref"
);
document_id!(
    /// A geometric relation (tangent, symmetric) between referenced objects.
    RelationId,
    "rel"
);
document_id!(
    /// A reusable part definition (component).
    PartDefinitionId,
    "part"
);
document_id!(
    /// One placement of a part definition; also `DerivedOwner::Part`.
    PartInstanceId,
    "inst"
);
document_id!(
    /// A sheet of stock with its own size and material.
    PlateId,
    "plate"
);
