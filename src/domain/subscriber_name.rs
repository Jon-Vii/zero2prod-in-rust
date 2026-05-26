pub struct SubscriberName(String);

impl SubscriberName {
    pub fn parse(value: String) -> Result<Self, String> {
        if value.trim().is_empty() {
            Err(format!("{value} is not a valid subscriber name"))
        } else {
            Ok(Self(value))
        }
    }
}

impl AsRef<str> for SubscriberName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::SubscriberName;
    use fake::{Fake, faker::name::en::Name};
    use quickcheck_macros::quickcheck;

    #[test]
    fn a_valid_name_is_parsed_successfully() {
        let name = "le guin".to_string();
        let parsed = SubscriberName::parse(name.clone()).expect("failed to parse valid name");

        assert_eq!(parsed.as_ref(), name);
    }

    #[test]
    fn an_empty_name_is_rejected() {
        let name = "".to_string();

        assert!(SubscriberName::parse(name).is_err());
    }

    #[test]
    fn a_whitespace_only_name_is_rejected() {
        let name = "   ".to_string();

        assert!(SubscriberName::parse(name).is_err());
    }

    #[quickcheck]
    fn valid_names_are_parsed_successfully(_: ()) -> bool {
        let name: String = Name().fake();

        SubscriberName::parse(name).is_ok()
    }
}
