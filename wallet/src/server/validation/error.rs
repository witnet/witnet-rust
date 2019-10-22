use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ValidationErrors(Vec<(String, String)>);

impl ValidationErrors {
    pub fn extend(&mut self, other: ValidationErrors) -> &mut Self {
        self.0.extend(other.0);
        self
    }
}

impl From<Vec<(String, String)>> for ValidationErrors {
    fn from(errors: Vec<(String, String)>) -> Self {
        ValidationErrors(errors)
    }
}
