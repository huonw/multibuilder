use git::Sha;

pub enum BuildInstruction {
    BuildHash(Sha)
}

/// Where the result of the build is. Designed to be extended for
/// distributed building.
pub enum BuiltLocation {
    Local(Path),
}

pub enum BuildResult {
    Success(BuiltLocation, Sha),
    Failure(Sha)
}
