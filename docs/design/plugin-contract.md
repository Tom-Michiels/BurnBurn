# Plugincontract `burnburn-plugin/1` (X06, beslissing B-5)

Alleen het contract. Er zijn **geen plugins in v1** en de applicatie heeft **geen laadmechanisme**: niets start een
plugin. Code: `crates/burnburn-cli/src/plugin_manifest.rs`; tests: `crates/burnburn-cli/tests/{plugin,plugin_protocol}.rs`;
schema: `docs/design/plugin-manifest.schema.json` (een test houdt het gelijk aan de validator).

## Manifest

JSON met `api` (exact `burnburn-plugin/1`; een andere versie geeft `UnsupportedApi`, ook als de rest van het schema
onbekend is), `name` (`[a-z0-9-]`, 1-64), `version`, `kind` (`generator`, `importer`, `exporter`), `command` (programma en
argumenten, buiten het proces gestart met timeout) en `capabilities`:

- `files`: `none` of `host-provided` (alleen de invoer die de host meegeeft);
- `network`: moet `false` zijn; `true` is `NetworkAccessForbidden`;
- `machine`: moet `false` zijn; `true` is `MachineAccessForbidden`.

Onbekende velden worden geweigerd. `PluginManifest::access_summary()` geeft de rechten voor het Uitbreidingen-paneel
(capabilities, API-versie, toegangsrechten); machinetoegang kan daar nooit in staan.

## Protocol (JSON Lines over stdout van de plugin)

Eén JSON-object per regel, UTF-8, elke regel maximaal 1 MiB, maximaal 100 000 regels, maximaal 1 000 000 punten.
Eerste regel `hello` (`api`, `name`, `version` gelijk aan het manifest). Daarna `progress` (`fraction` 0..1) en, voor
generator/importer, `paths` (`paths: [{closed, points: [[x_mm, y_mm], ...]}]`, eindige getallen). De laatste regel is `done`.

| Situatie | Resultaat |
|---|---|
| `error`-regel (`code`, `message`) | `PluginFailed`; geen gedeeltelijk resultaat |
| ontbrekende `hello` / andere naam of versie | `MissingHello` / `HelloMismatch`; andere API: `UnsupportedApi` |
| bericht van type machine, job, upload, start, send, connect, frame, jog, home, stop, network, exec | `ForbiddenMessage` |
| ander onbekend type | `UnknownMessage` |
| `paths` van een exporter | `KindMismatch` |
| geen `done` / regels na `done` | `Truncated` / `TrailingData` |
| geen geldige JSON, geen object, ongeldige waarden, te lang | `Malformed` / `LineTooLong` |

Het resultaattype `PluginOutput` bevat alleen geometrie (en voortgang). Een plugin kan geen job, machine of upload
benoemen en dus geen bevroren job vervangen; een mislukte run laat document en geschiedenis ongemoeid.

## Open

- Het starten, time-outen en afschermen van het proces (en de bestandsrechten `host-provided` afdwingen) is er niet; dat is
  werk voor een latere versie en vereist een eigen beveiligingsontwerp.
- Exporterberichten (bestanden terug naar de host) zijn nog niet gedefinieerd.
- Het contract hoort op termijn in een eigen `burnburn-plugin-api`-crate (nu in de CLI-crate).
