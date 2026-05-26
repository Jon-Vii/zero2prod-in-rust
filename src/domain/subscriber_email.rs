use validator::ValidateEmail;

pub struct SubscriberEmail(String);

impl SubscriberEmail {
    pub fn parse(value: String) -> Result<Self, String> {
        if value.validate_email() {
            Ok(Self(value))
        } else {
            Err(format!("{value} is not a valid subscriber email"))
        }
    }
}

impl AsRef<str> for SubscriberEmail {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::SubscriberEmail;
    use fake::{Fake, faker::internet::en::SafeEmail};
    use quickcheck_macros::quickcheck;

    #[test]
    fn a_valid_email_is_parsed_successfully() {
        let email = "ursula_le_guin@gmail.com".to_string();
        let parsed = SubscriberEmail::parse(email.clone()).expect("failed to parse valid email");

        assert_eq!(parsed.as_ref(), email);
    }

    #[test]
    fn an_empty_email_is_rejected() {
        let email = "".to_string();

        assert!(SubscriberEmail::parse(email).is_err());
    }

    #[test]
    fn an_email_without_at_symbol_is_rejected() {
        let email = "not-an-email".to_string();

        assert!(SubscriberEmail::parse(email).is_err());
    }

    #[test]
    fn an_email_without_a_domain_is_rejected() {
        let email = "ursula@".to_string();

        assert!(SubscriberEmail::parse(email).is_err());
    }

    #[quickcheck]
    fn valid_emails_are_parsed_successfully(_: ()) -> bool {
        let email: String = SafeEmail().fake();

        SubscriberEmail::parse(email).is_ok()
    }
}
