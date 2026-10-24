//! Small stopword lists used when deriving anchors automatically and to give
//! function words a low weight in the aligner. Only words of four letters or
//! more matter here, because shorter words are never anchor candidates in the
//! first place. Entries are in `normalize`d form: lowercase, accents folded,
//! apostrophes dropped ("être" → "etre", "c'est" → "cest").

use crate::Lang;

const NL: &[&str] = &[
    "aan", "achter", "alle", "alleen", "allen", "alles", "als", "altijd", "ander", "andere", "anders",
    "beetje", "beide", "ben", "bent", "bij", "binnen", "boven", "buiten", "daar", "daarom", "dan",
    "dat", "deze", "dit", "doen", "doet", "door", "dus", "echt", "een", "eens", "eerst", "elk", "elke",
    "erg", "even", "geen", "geweest", "gewoon", "goed", "haar", "had", "hadden", "heb", "hebben",
    "heeft", "heel", "hem", "hen", "het", "hier", "hoe", "hun", "iemand", "iets", "ik", "jaar",
    "jij", "jou", "jouw", "juist", "jullie", "kan", "kon", "kunnen", "kunt", "laat", "lang",
    "later", "liever", "maar", "mag", "meer", "meest", "men", "met", "mij", "mijn", "misschien",
    "moet", "moeten", "mogelijk", "naar", "net", "niet", "niets", "nog", "nooit", "nou", "omdat",
    "onder", "ons", "onze", "ook", "over", "precies", "reeds", "rond", "sinds", "soms", "steeds",
    "tegen", "terwijl", "toch", "toen", "tot", "tussen", "uit", "vaak", "van", "vanaf", "vandaag",
    "veel", "verder", "voor", "vooral", "waar", "waarom", "want", "waren", "was", "wat", "weer",
    "weet", "weinig", "wel", "welke", "werd", "werden", "weten", "wie", "wil", "willen", "word",
    "worden", "wordt", "zeer", "zelf", "zich", "zijn", "zoals", "zonder", "zou", "zouden", "zulke",
    "zullen", "hele", "deze", "die", "dit", "eigenlijk", "gaan", "gaat", "hebt", "kijken", "komt",
    "komen", "maken", "maakt", "mensen", "vandaag", "vorige", "week", "andere", "iemand",
    "eerste", "tweede", "derde", "vierde", "vijfde", "laatste", "volgende", "dieper", "jaren",
    "loopt", "lijkt", "gaat", "hoe", "hier", "daarom", "twee", "drie", "vier", "vijf", "zes", "zeven",
    "acht", "negen", "tien", "honderd", "duizend",
];

const EN: &[&str] = &[
    "about", "above", "after", "again", "against", "also", "always", "another", "anyone", "anything",
    "around", "because", "been", "before", "being", "below", "between", "both", "cannot", "could",
    "does", "doing", "down", "during", "each", "either", "else", "even", "ever", "every", "everyone",
    "everything", "from", "further", "gets", "getting", "going", "gone", "have", "having", "here",
    "hers", "herself", "himself", "into", "itself", "just", "keep", "know", "later", "less", "like",
    "look", "made", "make", "many", "maybe", "might", "more", "most", "much", "must", "myself",
    "need", "never", "next", "once", "only", "other", "others", "ought", "ours", "ourselves", "over",
    "people", "quite", "rather", "really", "same", "shall", "should", "since", "some", "something",
    "still", "such", "sure", "take", "than", "that", "their", "theirs", "them", "themselves", "then",
    "there", "these", "they", "thing", "things", "think", "this", "those", "though", "through",
    "today", "toward", "under", "until", "upon", "used", "using", "very", "want", "week", "were",
    "what", "when", "where", "which", "while", "whom", "whose", "will", "with", "within", "without",
    "would", "year", "your", "yours", "yourself", "yourselves",
    "first", "second", "third", "fourth", "fifth", "last", "three", "four", "five", "seven", "eight",
    "nine", "hundred", "thousand",
];

const DE: &[&str] = &[
    "aber", "alle", "allem", "allen", "aller", "alles", "also", "andere", "anderen", "anders",
    "auch", "bitte", "bleibt", "damit", "dabei", "dann", "darauf", "darum", "dass", "davon", "dazu",
    "dein", "deine", "denn", "dessen", "diese", "diesem", "diesen", "dieser", "dieses", "doch",
    "dort", "durch", "eben", "eigentlich", "eine", "einem", "einen", "einer", "eines", "einfach",
    "etwas", "euch", "euer", "eure", "ganz", "gegen", "genau", "gerade", "geht", "gibt", "habe",
    "haben", "hast", "hatte", "hatten", "heute", "hier", "hinter", "ihnen", "ihre", "ihrem",
    "ihren", "ihrer", "immer", "jede", "jedem", "jeden", "jeder", "jedes", "jetzt", "kann",
    "kannst", "kein", "keine", "keinen", "konnen", "konnte", "lassen", "machen", "macht", "mehr",
    "mein", "meine", "meinen", "menschen", "mich", "muss", "mussen", "nach", "nicht", "nichts",
    "noch", "oder", "ohne", "schon", "sehr", "sein", "seine", "seinen", "selbst", "sich", "sind",
    "soll", "sollen", "sondern", "sowie", "uber", "unser", "unsere", "unter", "viel", "viele",
    "vielleicht", "waren", "warum", "weil", "welche", "welcher", "wenn", "werden", "wieder",
    "will", "wird", "wirklich", "wollen", "wurde", "wurden", "zwar", "zwischen", "jahr", "jahre",
    "woche", "erste", "ersten", "zweite", "dritte", "letzte", "nachste", "zwei", "drei", "vier",
    "funf", "sechs", "sieben", "acht", "neun", "zehn", "hundert", "tausend",
];

const FR: &[&str] = &[
    "ainsi", "alors", "apres", "aujourdhui", "aussi", "autre", "autres", "avant", "avec", "avez",
    "avoir", "avons", "beaucoup", "bien", "cela", "celle", "celui", "cest", "cette", "ceux",
    "chaque", "chez", "chose", "choses", "comme", "comment", "dans", "deja", "depuis", "donc",
    "dont", "elle", "elles", "encore", "entre", "etait", "etre", "fait", "faire", "faut", "gens",
    "ici", "jamais", "juste", "leur", "leurs", "lors", "mais", "meme", "mieux", "moins", "notre",
    "nous", "parce", "pendant", "peut", "peutetre", "peux", "plus", "pour", "pourquoi", "puis",
    "quand", "quel", "quelle", "quelles", "quels", "quelque", "quelques", "quil", "quils",
    "sans", "selon", "sera", "sont", "sous", "toujours", "tous", "tout", "toute",
    "toutes", "tres", "trop", "vers", "voici", "voila", "votre", "vous", "vraiment", "semaine",
    "annee", "premier", "premiere", "deuxieme", "troisieme", "dernier", "derniere", "prochain",
    "prochaine", "deux", "trois", "quatre", "cinq", "sept", "huit", "neuf", "cent", "mille",
];

const ES: &[&str] = &[
    "ahora", "algo", "alguien", "algun", "alguna", "algunas", "alguno", "algunos", "ante",
    "antes", "aqui", "aunque", "bien", "cada", "casi", "como", "contra", "cosa", "cosas", "cual",
    "cuando", "desde", "despues", "donde", "durante", "ella", "ellas", "ellos", "entonces",
    "entre", "eres", "esas", "esos", "esta", "estan", "estar", "estas", "este", "esto", "estos",
    "estoy", "estamos", "fueron", "gente", "habia", "haber", "hace", "hacer", "hasta", "hemos",
    "luego", "mientras", "mismo", "misma", "mucha", "muchas", "mucho", "muchos", "nada",
    "nosotros", "nuestra", "nuestro", "otra", "otras", "otro", "otros", "para", "pero", "poco",
    "porque", "puede", "pueden", "quien", "quienes", "sera", "sido", "siempre", "sino", "sobre",
    "solo", "somos", "tambien", "tampoco", "tanto", "tener", "tengo", "tiene", "tienen", "todas",
    "todo", "todos", "unas", "unos", "usted", "ustedes", "vamos", "veces", "semana", "primero",
    "primera", "segundo", "segunda", "tercero", "ultimo", "ultima", "siguiente", "tres",
    "cuatro", "cinco", "seis", "siete", "ocho", "nueve", "diez", "cien", "mil",
];

const PT: &[&str] = &[
    "agora", "ainda", "algo", "alguem", "algum", "alguma", "algumas", "alguns", "antes", "apenas",
    "aqui", "assim", "atraves", "cada", "como", "contra", "coisa", "coisas", "depois", "desde",
    "dessa", "desse", "desta", "deste", "disso", "disto", "durante", "elas", "eles", "entao",
    "entre", "essa", "essas", "esse", "esses", "esta", "estao", "estar", "estas", "este", "estes",
    "isso", "isto", "gente", "hoje", "mais", "mesmo", "mesma", "muito", "muita", "muitas",
    "muitos", "nada", "nossa", "nosso", "onde", "outra", "outras", "outro", "outros", "para",
    "pela", "pelas", "pelo", "pelos", "pessoas", "porque", "pode", "podem", "pouco", "quando",
    "quem", "qual", "quais", "seja", "sempre", "sobre", "somos", "sido", "seus", "suas", "talvez",
    "tambem", "tanto", "tem", "tenho", "temos", "todas", "todo", "todos", "tudo", "voce", "voces",
    "vamos", "vezes", "semana", "primeiro", "primeira", "segundo", "segunda", "terceiro",
    "ultimo", "ultima", "proximo", "proxima", "tres", "quatro", "cinco", "seis", "sete", "oito",
    "nove", "cem", "cento", "mil",
];

pub fn is_stopword(lang: Lang, word: &str) -> bool {
    let list = match lang {
        Lang::Nl => NL,
        Lang::En => EN,
        Lang::De => DE,
        Lang::Fr => FR,
        Lang::Es => ES,
        Lang::Pt => PT,
    };
    list.contains(&word)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchors::normalize;

    #[test]
    fn lists_are_in_normalized_form() {
        for lang in Lang::ALL {
            let list = match lang {
                Lang::Nl => NL,
                Lang::En => EN,
                Lang::De => DE,
                Lang::Fr => FR,
                Lang::Es => ES,
                Lang::Pt => PT,
            };
            for w in list {
                assert_eq!(&normalize(w), w, "{lang:?}: {w}");
            }
        }
    }

    #[test]
    fn accented_words_match_after_normalizing() {
        assert!(is_stopword(Lang::Fr, &normalize("Être")));
        assert!(is_stopword(Lang::Fr, &normalize("c'est")));
        assert!(is_stopword(Lang::De, &normalize("über")));
        assert!(is_stopword(Lang::Es, &normalize("también")));
        assert!(is_stopword(Lang::Pt, &normalize("você")));
        assert!(!is_stopword(Lang::De, &normalize("Kaffeebohnen")));
    }
}
