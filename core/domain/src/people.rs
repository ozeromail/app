//! The people the user writes to: found by what they type in "To", named as in their contacts.

use std::collections::HashMap;

use crate::sort::is_robot;

/// Someone in the phone's contacts, as the app read them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    /// What the phone calls the contact by, to open it there.
    pub key: String,
    pub name: String,
    pub emails: Vec<String>,
}

/// An address the user's mail went to or came from, all its messages together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Correspondent {
    pub email: String,
    /// The name its newest message gives it.
    pub name: Option<String>,
    /// Messages the user sent to it.
    pub sent: u32,
    /// Messages from it to the user, people's mail only: not notifications or newsletters.
    pub received: u32,
    /// The newest of them, in seconds.
    pub last: i64,
}

/// Someone found in a server's address book, as an Exchange company directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub name: String,
    pub email: String,
    /// Their job title or office, if the directory says.
    pub title: Option<String>,
}

/// Someone the user can write to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    /// The name in the contacts, else in the mail, else the address.
    pub name: String,
    pub email: String,
    /// The contact's key, when the address is in the contacts.
    pub contact: Option<String>,
}

/// A person with all their addresses, for the card a tap on their name opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub name: String,
    /// The tapped address first, then the contact's others, most used first.
    pub emails: Vec<String>,
    pub contact: Option<String>,
}

/// Messages the user sent count this many times more than those they got.
const SENT_WEIGHT: f64 = 3.0;
/// After this many days without mail, an address counts half as much.
const HALF_DAYS: f64 = 30.0;

/// Everyone the user can write to, most written to first.
#[derive(Debug, Clone, Default)]
pub struct People {
    known: Vec<Known>,
    /// The contacts' addresses, by lowercase address, with the contact they belong to.
    contacts: HashMap<String, usize>,
    cards: Vec<Contact>,
}

#[derive(Debug, Clone)]
struct Known {
    person: Person,
    score: f64,
    /// Lowercase, ё as е: the name's words, the address and the parts of it.
    words: Vec<String>,
}

impl People {
    /// From the user's mail, the phone's contacts and the user's own addresses, which are left
    /// out, at `now` (seconds). Robot addresses, as `noreply@`, are left out too.
    pub fn new(mail: Vec<Correspondent>, contacts: Vec<Contact>, own: &[String], now: i64) -> Self {
        let own: Vec<String> = own.iter().map(|e| e.to_lowercase()).collect();
        let wanted = |email: &str| !own.iter().any(|o| o == email) && !is_robot(email);
        let mut by_email: HashMap<String, usize> = HashMap::new();
        let mut known: Vec<Known> = Vec::new();
        for c in mail {
            let email = c.email.to_lowercase();
            if !wanted(&email) || by_email.contains_key(&email) {
                continue;
            }
            let days = (now - c.last).max(0) as f64 / 86_400.0;
            let score = (SENT_WEIGHT * f64::from(c.sent) + f64::from(c.received))
                / (1.0 + days / HALF_DAYS);
            let name = c
                .name
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| c.email.clone());
            by_email.insert(email, known.len());
            known.push(Known {
                person: Person {
                    name,
                    email: c.email,
                    contact: None,
                },
                score,
                words: vec![],
            });
        }
        let mut index = HashMap::new();
        for (at, contact) in contacts.iter().enumerate() {
            for email in &contact.emails {
                let lower = email.to_lowercase();
                if !wanted(&lower) {
                    continue;
                }
                index.entry(lower.clone()).or_insert(at);
                let person = Person {
                    name: contact.name.clone(),
                    email: email.clone(),
                    contact: Some(contact.key.clone()),
                };
                match by_email.get(&lower) {
                    Some(&i) if known[i].person.contact.is_none() => known[i].person = person,
                    Some(_) => {}
                    None => {
                        by_email.insert(lower, known.len());
                        known.push(Known {
                            person,
                            score: 0.0,
                            words: vec![],
                        });
                    }
                }
            }
        }
        for k in &mut known {
            k.words = words(&k.person);
        }
        known.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| a.person.name.cmp(&b.person.name))
        });
        Self {
            known,
            contacts: index,
            cards: contacts,
        }
    }

    /// Up to `limit` people with every word of `text` as the start of a word of their name or
    /// address, or with an address that starts with it; most written to first, case and ё
    /// aside. Nobody for empty text.
    pub fn find(&self, text: &str, limit: usize) -> Vec<Person> {
        let typed = fold(text.trim());
        let parts: Vec<&str> = typed.split_whitespace().collect();
        if parts.is_empty() {
            return vec![];
        }
        self.known
            .iter()
            .filter(|k| {
                k.words[0].starts_with(&typed)
                    || parts
                        .iter()
                        .all(|p| k.words.iter().any(|w| w.starts_with(p)))
            })
            .take(limit)
            .map(|k| k.person.clone())
            .collect()
    }

    /// The person behind `email`, with `name` as the mail gives it when the contacts do not.
    pub fn card(&self, email: &str, name: Option<&str>) -> Card {
        let Some(&at) = self.contacts.get(&email.to_lowercase()) else {
            let name = name
                .filter(|n| !n.trim().is_empty())
                .unwrap_or(email)
                .to_owned();
            return Card {
                name,
                emails: vec![email.to_owned()],
                contact: None,
            };
        };
        let contact = &self.cards[at];
        let rank = |e: &String| {
            self.known
                .iter()
                .position(|k| k.person.email.eq_ignore_ascii_case(e))
                .unwrap_or(usize::MAX)
        };
        let mut others: Vec<String> = contact
            .emails
            .iter()
            .filter(|e| !e.eq_ignore_ascii_case(email))
            .cloned()
            .collect();
        others.sort_by_key(rank);
        let mut emails = vec![email.to_owned()];
        emails.extend(others);
        Card {
            name: contact.name.clone(),
            emails,
            contact: Some(contact.key.clone()),
        }
    }
}

/// Lower case with ё as е, as people type it.
fn fold(text: &str) -> String {
    text.to_lowercase().replace('ё', "е")
}

/// The address first, then the words of the name and of the address.
fn words(person: &Person) -> Vec<String> {
    let email = fold(&person.email);
    let mut words = vec![email.clone()];
    words.extend(
        fold(&person.name)
            .split(|c: char| !c.is_alphanumeric())
            .chain(email.split(|c: char| !c.is_alphanumeric()))
            .filter(|w| !w.is_empty())
            .map(str::to_owned),
    );
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;

    fn mail(email: &str, name: &str, sent: u32, received: u32, last: i64) -> Correspondent {
        Correspondent {
            email: email.into(),
            name: (!name.is_empty()).then(|| name.into()),
            sent,
            received,
            last,
        }
    }

    fn contact(key: &str, name: &str, emails: &[&str]) -> Contact {
        Contact {
            key: key.into(),
            name: name.into(),
            emails: emails.iter().map(|e| (*e).into()).collect(),
        }
    }

    fn emails(found: Vec<Person>) -> Vec<String> {
        found.into_iter().map(|p| p.email).collect()
    }

    #[test]
    fn ranks_who_the_user_writes_to_first() {
        let people = People::new(
            vec![
                mail("anna@example.com", "Anna Lee", 0, 9, 100 * DAY),
                mail("oleg@example.com", "Oleg Smirnov", 4, 0, 100 * DAY),
                mail("olga@example.com", "Olga", 1, 0, 10 * DAY),
            ],
            vec![],
            &[],
            100 * DAY,
        );
        assert_eq!(
            emails(people.find("o", 5)),
            ["oleg@example.com", "olga@example.com"]
        );
        // Four sent count as twelve received; old mail counts less.
        assert_eq!(
            emails(people.find("example", 5)),
            ["oleg@example.com", "anna@example.com", "olga@example.com"]
        );
    }

    #[test]
    fn finds_by_any_word_case_and_yo_aside() {
        let people = People::new(
            vec![
                mail("p.semenov@example.com", "Пётр Семёнов", 1, 0, 0),
                mail("lee@example.com", "Lee, Anna", 1, 0, 0),
            ],
            vec![],
            &[],
            0,
        );
        assert_eq!(emails(people.find("ПЕТР", 5)), ["p.semenov@example.com"]);
        assert_eq!(emails(people.find("сем пе", 5)), ["p.semenov@example.com"]);
        assert_eq!(emails(people.find("anna", 5)), ["lee@example.com"]);
        assert_eq!(emails(people.find("lee@exa", 5)), ["lee@example.com"]);
        assert!(people.find("  ", 5).is_empty());
        assert!(people.find("emenov", 5).is_empty());
    }

    #[test]
    fn contacts_name_people_and_add_those_never_written_to() {
        let people = People::new(
            vec![mail("patimat@example.com", "Patimat M.", 2, 5, 0)],
            vec![
                contact("1", "Мама", &["patimat@example.com"]),
                contact("2", "Олеся Ким", &["olesya@example.com"]),
            ],
            &[],
            0,
        );
        let mom = &people.find("мама", 5)[0];
        assert_eq!(
            (mom.name.as_str(), mom.contact.as_deref()),
            ("Мама", Some("1"))
        );
        assert_eq!(people.find("patimat", 5).len(), 1);
        assert_eq!(emails(people.find("олеся", 5)), ["olesya@example.com"]);
        assert_eq!(people.card("PATIMAT@example.com", None).name, "Мама");
    }

    #[test]
    fn leaves_out_the_user_and_robots() {
        let people = People::new(
            vec![
                mail("me@example.com", "Me", 9, 9, 0),
                mail("no-reply@shop.example", "Shop", 0, 9, 0),
                mail("Do_Not_Reply@bank.example", "Bank", 0, 9, 0),
                mail("nora@example.com", "Nora", 1, 0, 0),
            ],
            vec![],
            &["ME@example.com".into()],
            0,
        );
        assert_eq!(emails(people.find("example", 5)), ["nora@example.com"]);
        assert!(people.find("shop", 5).is_empty());
    }

    #[test]
    fn a_card_has_all_the_contacts_addresses_most_used_first() {
        let people = People::new(
            vec![
                mail("oleg@work.example", "Oleg", 9, 0, 0),
                mail("oleg@home.example", "Oleg", 1, 0, 0),
            ],
            vec![contact(
                "7",
                "Олег Смирнов",
                &["oleg@old.example", "oleg@home.example", "oleg@work.example"],
            )],
            &[],
            0,
        );
        let card = people.card("oleg@home.example", Some("Oleg"));
        assert_eq!(card.name, "Олег Смирнов");
        assert_eq!(
            card.emails,
            ["oleg@home.example", "oleg@work.example", "oleg@old.example"]
        );
        assert_eq!(card.contact.as_deref(), Some("7"));
        let stranger = people.card("anna@example.com", None);
        assert_eq!(
            (stranger.name.as_str(), stranger.contact),
            ("anna@example.com", None)
        );
    }
}
