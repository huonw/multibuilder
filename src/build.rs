use git::Sha;
use std::fmt;

#[deriving(Show)]
pub enum BuildInstruction {
    BuildHash(Sha)
}

/// Where the result of the build is. Designed to be extended for
/// distributed building.
pub enum BuiltLocation {
    Local(Path),
}

impl fmt::Show for BuiltLocation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Local(ref p) = *self;
        write!(f, "Local({})", p.display())
    }
}

#[deriving(Show)]
pub enum BuildResult {
    Success(BuiltLocation, Sha),
    Failure(Sha)
}
