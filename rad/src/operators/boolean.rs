use crate::types::boolean::RadonBoolean;
use crate::types::RadonType;

pub fn negate(input: &RadonBoolean) -> RadonBoolean {
    RadonBoolean::from(!input.value())
}
