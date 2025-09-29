// Validation tests for term construction

#[cfg(test)]
mod tests {
    use ttt::core::Term;

    #[test]
    fn test_identity_function() {
        // λx. x
        let identity = Term::lambda(Term::var(0));

        match identity {
            Term::Lambda(ref body) => {
                assert_eq!(**body, Term::Var(0));
            }
            _ => panic!("Expected Lambda"),
        }
    }

    #[test]
    fn test_identity_type() {
        // Π(A : Type). A → A
        let id_type = Term::pi(
            Term::type_0(),
            Term::pi(
                Term::var(0),
                Term::var(1)
            )
        );

        match id_type {
            Term::Pi(ref dom, ref cod) => {
                assert_eq!(**dom, Term::Universe(ttt::core::Level(0)));
                assert!(matches!(**cod, Term::Pi(_, _)));
            }
            _ => panic!("Expected Pi type"),
        }
    }

    #[test]
    fn test_application() {
        // (λx. x) y
        let app = Term::app(
            Term::lambda(Term::var(0)),
            Term::var(1)
        );

        match app {
            Term::App(ref f, ref x) => {
                assert!(matches!(**f, Term::Lambda(_)));
                assert_eq!(**x, Term::Var(1));
            }
            _ => panic!("Expected Application"),
        }
    }
}