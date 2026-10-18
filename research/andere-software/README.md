# Andere software voor Ruida — vergelijking en ontwerpideeën

Onderzoeksdatum: **17 oktober 2026**. Dit dossier vult het [LightBurn-onderzoek](../lightburn/README.md) en het [Ruida-protocolonderzoek](../rudia/README.md) aan. Taal: Nederlands, met herkenbare oorspronkelijke functienamen.

## Hoofdbevinding

Voor BurnBurn is een combinatie van ideeën het interessantst: de expliciete productievoorbereiding van VisiCut, de bewerkingen en wachtrij van MeerK40t, de padcontrole van RDWorks en de configureerbare werkstroom van Rayforge. Voor camera-ondersteuning verdienen Ruida's visionpakketten een afzonderlijke ontwerpstudie. Dit is een **ontwerpconclusie**, geen rangschikking op gemeten machineprestaties.

Er zijn vier alternatieve complete toepassingen onderzocht: **RDWorks V8, MeerK40t, VisiCut en Rayforge**. Daarnaast zijn **MantiSoft**, de **Ruida-visionfamilie**, **RDNest** en **Ruida Protocol Analyzer** bekeken. Die laatste groepen hebben een andere rol of een nog onzekere aansluiting op een standaard RDC6442/6445. LightBurn blijft de reeds onderzochte referentie.

## Leesroute

| Bestand | Inhoud |
|---|---|
| [01-softwareprofielen.md](01-softwareprofielen.md) | Functies, werkstromen, sterke punten en beperkingen per pakket |
| [02-featurevergelijking.md](02-featurevergelijking.md) | Vergelijkingsmatrix, compatibiliteit en keuze per gebruikssituatie |
| [03-beste-ideeen-voor-burnburn.md](03-beste-ideeen-voor-burnburn.md) | Twaalf uitgewerkte ideeën met werking, datamodel en acceptatiecriterium |
| [04-validatie-en-bouwvolgorde.md](04-validatie-en-bouwvolgorde.md) | Prioriteiten, concrete proefontwerpen en resterende onzekerheden |
| [05-bronnen-en-methode.md](05-bronnen-en-methode.md) | Bronregister, versies, bewijsgrenzen en vindplaatsen |

## Wat is werkelijk vastgesteld?

Dit is bronnenonderzoek op basis van fabrikantdocumentatie, projectdocumentatie, release notes en gericht gelezen openbare broncode. Er is in dit vervolg **geen software geïnstalleerd of interactief bediend en geen controller aangesloten**. De eerdere LightBurn-appwaarnemingen blijven uitsluitend bewijs voor dat eerdere dossier.

Gebruikte labels:

- **DOC:** de maker beschrijft de functie; geen onafhankelijke praktijktest.
- **CODE:** de bekeken broncode bevat de route of instelling; geen garantie voor alle apparaten.
- **CAT:** het pakket staat in een officiële catalogus; de naam bewijst geen functionaliteit of compatibiliteit.
- **ONTWERP:** eigen analyse of voorstel voor BurnBurn.
- **OPEN:** nog te verifiëren; betekent niet dat de functie ontbreekt.

Een algemene productfunctie is niet automatisch een Ruida-functie. Zo noemt Rayforge brede rotarymogelijkheden, maar zijn specifieke Ruida-documentatie sluit rotary nog uit. De projectdocumentatie noemt Ruida bovendien experimenteel. [Rayforge firmwarecompatibiliteit](https://rayforge.org/docs/reference/firmware/).

## Meest waardevolle aanknopingspunten

1. **Bewerkingen los van kleuren en geometrie.** Een vorm kan eerst worden gegraveerd en daarna gesneden zonder kopieën in het ontwerp.
2. **Een herleidbare job.** Preview, RD-export en upload moeten voortkomen uit dezelfde bevroren bewerkingsplanning.
3. **Een afzonderlijke productiewachtrij.** Voorbereid, verstuurd en fysiek voltooid zijn verschillende toestanden.
4. **Controleerbare automatische optimalisatie.** Laat de gebruiker zien waarom de volgorde verandert en welke handmatige keuzes vaststaan.
5. **Camera als meetinstrument.** Bewaar registratie, kwaliteit en herkenningskenmerken afzonderlijk van het snijpad.
6. **Machineprofielen met bewijs per functie.** Toon ondersteuning per model, firmware en transport, inclusief onbekende mogelijkheden.

De onderbouwing per product staat in de profielen; de concrete eigen uitwerking staat in de ideeëncatalogus.
