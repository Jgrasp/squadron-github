# Journal des modifications

## 0.4.0 · 2026-11-20

- Ouvrir depuis Squadron une pull request pour la branche poussée d’un agent, en brouillon quand le dépôt le permet, avec un message clair quand une pull request est déjà ouverte ou que la branche n’a aucun nouveau commit

## 0.3.0 · 2026-11-20

- La fusion d’une pull request utilise la méthode choisie dans Squadron, parmi celles que le dépôt et les règles de sa branche de base autorisent
- Squadron indique avant une fusion si GitHub supprimera la branche source, d’après le réglage « Automatically delete head branches » du dépôt, et quand une branche protégée est conservée

## 0.2.0 · 2026-11-19

- Les runs GitHub Actions indiquent la pull request qu’ils vérifient : Squadron affiche son numéro et son titre sur le run et le range dans « Checks de PR »

## 0.1.1 · 2026-11-19

- Les runs GitHub Actions s’affichent de nouveau pour les projets qui passent par la GitHub CLI
- La fusion d’une pull request prend une méthode autorisée par le dépôt et les règles de sa branche de base, quand votre méthode par défaut ne l’est pas

## 0.1.0 · 2026-11-18

- Les pull requests de vos projets GitHub dans Squadron : celle de la branche de chaque agent, celles à relire, les vôtres et les autres, avec leurs checks, leurs relecteurs et leurs conflits
- Des revues avec commentaires de ligne, résumé et verdict, des réponses aux remarques, des fils résolus affichés et résolus depuis Squadron, des fusions avec la méthode par défaut du dépôt, et le changement du titre et de la description
- Les runs GitHub Actions de chaque projet, avec une notification à la fin de chacun
- Des puces pour les liens de pull requests GitHub dans les conversations
- Un compte GitHub par propriétaire, avec un jeton à granularité fine vérifié avant d’être enregistré ; sans compte, la GitHub CLI prend le relais
