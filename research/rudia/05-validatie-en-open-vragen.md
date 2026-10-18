# Validatie en open vragen

## Wat al vaststaat

De lokale Line-, Fill- en Grayscale-fixtures bevestigen een subset van de RD-bestandsstructuur, scrambling, bewegingen, parameters en eindrecords. De commandoregistry bundelt 200 openbare constanten en acht geheugenadressen, zonder universele supportclaim. Fabrikant- en LightBurn-documentatie bevestigen de genoemde gebruikersroutes; de UDP-details zijn gebaseerd op openbare implementaties.

Het exacte controllerbord, firmware, USB-device en werkbereik van jouw machine zijn nog onbekend. Daardoor zijn compatibiliteit, bestandsopslag, timing en uitvoergedrag niet op jouw hardware bevestigd. Firmware-updates en een generieke USB-driver vallen buiten de bewezen subset.

## Benodigde gegevens voor de doelmachine

| Gegeven | Waarom nodig |
|---|---|
| Volledig hoofdprintmodel en revisie | Protocolfamilie en juiste hardwarehandleiding |
| Firmware en paneelversie | Functies en variantgedrag |
| Huidige software en versie | Werkende referentie-export en verbinding |
| IP-adres of USB-identificatie | Transportconfiguratie |
| Werkbereik en nulpunthoek | Plaatsing en bounds |
| Assen, rotary en aantal lasers | Capabilities en kanaalkeuze |
| Export van bestaande instellingen | Kalibratie en herstel |

## Gerichte hardwareproeven

1. Lees model/status met een bestaande werkende toepassing en leg de antwoorden vast.
2. Maak een kleine RD-job voor het echte bed en origin; vergelijk de offline decode met de preview.
3. Capture een **Send**-actie inclusief bestandsnaam, initialisatie, chunks, antwoorden en opslagverificatie.
4. Capture afzonderlijk start, pauze, hervatten en stop. Vergelijk hun context met bestandsrecords.
5. Bevestig maximale chunkgrootte, timeouts en reacties op een afgebroken upload.
6. Test verloren ACK en duplicatie op emulator of gecontroleerde bench voordat automatisch herstel wordt toegevoegd.
7. Onderzoek USB apart: VID/PID, driver, endpoints of seriële instellingen en request/response-framing.

Een ontvangstbevestiging, zichtbare bestandsnaam en afgeronde fysieke job zijn drie verschillende bewijzen. Rapporteer deze afzonderlijk in een eigen applicatie.

## Offline verificatie

`evidence/validation-results.txt` bevat de in dit dossier uitgevoerde controles. Deze controleren de analysebestanden, lokale links, JSON en pakketopbouw. Zij zeggen niets over fysieke machinecompatibiliteit.

De gekopieerde protocolhoofdstukken blijven een momentopname van het LightBurn-onderzoek van 17 oktober 2026. Claims over vroeger uitgevoerde appinspectie en exports horen bij dat oorspronkelijke onderzoek; deze uitbreiding heeft de app niet opnieuw bediend.
