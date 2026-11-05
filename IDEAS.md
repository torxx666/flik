# Boîte à idées & Roadmap Flik

Ce document centralise les propositions, retours d'utilisateurs et pistes d'évolutions futures pour Flik.

---

## 1. Suivi des retours externes & Comparatif lnav

### Retour d'expérience
- **Disponibilité / Packaging** : Les critiques initiales concernant l'absence de binaire Windows ou de distribution officielle sont résolues en **v0.2.0** (binaires macOS signables, Linux musl x86_64/ARM64, Windows natif x86_64, et tap Homebrew `brew install torxx666/tap/flik`).
- **Benchmark sur gros volumes (fichiers de plusieurs Go)** :
  - **Flik** : Indexation d'offsets 64 bits en RAM (`Vec<u64>`), soit **strictement 8 octets par ligne**, 0 texte conservé en RAM, lecture directe sur disque via seek $O(1)$. Pour 10M de lignes (~2 Go) : ~80 Mo RAM. Pour 100M de lignes (~20 Go) : ~800 Mo RAM. Indexation initiale ultra-rapide par scan SIMD (`memchr`).
  - **lnav** : Parse les timestamps et génère une base SQLite virtuelle en RAM. Très puissant pour les requêtes SQL, mais peut saturer la RAM (plusieurs Go) et être très lent au démarrage sur les fichiers massifs.
  - *Action roadmap* : Documenter formellement ces métriques de RAM/débit dans la documentation et sur le site.

---

## 2. Pistes d'évolution & Spécifications

### 🎯 Priorité 1 — Quick Wins & Améliorations ciblées (v0.3.0)

#### A. Configuration globale & Formats personnalisés
- **Fichier de configuration** : `~/.config/flik/config.toml` (ou `%APPDATA%\flik\config.toml` sur Windows).
  - Personnalisation des raccourcis clavier.
  - Définition du thème de couleurs par défaut (palette syntaxique, surlignages).
  - Paramètres d'alerte par défaut (seuils d'erreurs, règles automatiques).
- **Formats personnalisés** : Dossier `~/.config/flik/formats/*.toml` ou `*.json` permettant aux utilisateurs de déclarer leurs propres regex et extracteurs de champs (ex: formats d'access log Apache/Nginx personnalisés, `auth.log`, formats d'entreprise propriétaires) sans recompiler Flik.

#### B. Statistiques sur l'intégralité du fichier
- **Situation actuelle** : Le calcul de `StatsSummary` échantillonne les 5 000 dernières lignes pour garantir un affichage instantané (< 15 ms).
- **Évolution** :
  - Conserver les 5 000 lignes par défaut pour l'ouverture immédiate.
  - Ajouter une touche <kbd>A</kbd> (*Analyze All*) dans le modal de statistiques.
  - Exécuter le scan complet dans un thread d'arrière-plan sans bloquer l'UI, avec une jauge de progression et affichage en temps réel des compteurs.

#### C. Autocomplétion enrichie sur les champs JSON
- **Situation actuelle** : Complétion disponible via `Ctrl+Space` / `Tab`, échantillonnée sur 500 lignes.
- **Évolution** :
  - Suggestions automatiques à la frappe dans la barre de recherche `/` et de filtres `+`.
  - Support des clés JSON imbriquées (ex: `user.profile.id`, `kubernetes.labels.app`).
  - Menu contextuel ou hint visuel plus explicite des valeurs fréquentes.

---

### 🚀 Priorité 2 — Multi-sources & Vues avancées

#### D. Affichage Multi-Fichiers (Deux modes complémentaires)
1. **Mode Split-pane (côte-à-côte)** :
   - Ouvrir deux ou plusieurs fichiers dans des panneaux verticaux ou horizontaux distincts.
   - Permet de comparer visuellement deux logs ou de garder un œil sur un fichier d'erreurs tout en observant un flux d'accès.
2. **Mode Fusion Chronologique (Timeline unifiée en parallèle)** :
   - Entremêlement ordonné de plusieurs fichiers de logs selon leur timestamp (k-way merge stream).
   - Une colonne ou un badge coloré identifie la source de chaque ligne (ex: `[nginx]`, `[auth]`, `[app]`).
   - Permet de suivre le déroulement temporel d'une requête ou d'un incident à travers plusieurs composants distribués.
   - *Note* : Ce mode est **en complément** du split-pane, pas un remplacement.

---

### 🔮 Priorité 3 — Écosystème & Extensibilité

#### E. Système d'extensions / plugins
- Possibilité d'attacher des transformateurs de logs ou des analyseurs légers (WASM ou scripts Lua).
- Intégration de hooks d'alertes externes (webhook Slack, notification bureau, export automatique).

