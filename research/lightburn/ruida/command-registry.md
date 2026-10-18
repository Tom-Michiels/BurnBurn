# Publieke Ruida-commandoregistry

Dit is een vindregister van 200 constanten uit MeerK40t op commit d511944e08788e88077727f682957759884ff5ca. Namen en parameterhints zijn upstreamidentificatie; hun semantiek en firmwareondersteuning zijn niet volledig door dit onderzoek bevestigd. Zie de JSON voor bronregels en de binaire-specificatie voor de gemeten subset.

| Hex raw | Symbool | Parameterhint uit bron |
|---|---|---|
| a5 53 00 | INTERFACE_FRAME | — |
| a5 50 02 | INTERFACE_PLUS_X_DOWN | — |
| a5 51 02 | INTERFACE_PLUS_X_UP | — |
| a5 50 01 | INTERFACE_MINUS_X_DOWN | — |
| a5 51 01 | INTERFACE_MINUS_X_UP | — |
| a5 50 03 | INTERFACE_PLUS_Y_DOWN | — |
| a5 51 03 | INTERFACE_PLUS_Y_UP | — |
| a5 50 04 | INTERFACE_MINUS_Y_DOWN | — |
| a5 51 04 | INTERFACE_MINUS_Y_UP | — |
| a5 50 0a | INTERFACE_PLUS_Z_DOWN | — |
| a5 51 0a | INTERFACE_PLUS_Z_UP | — |
| a5 50 0b | INTERFACE_MINUS_Z_DOWN | — |
| a5 51 0b | INTERFACE_MINUS_Z_UP | — |
| a5 50 0c | INTERFACE_PLUS_U_DOWN | — |
| a5 51 0c | INTERFACE_PLUS_U_UP | — |
| a5 50 0d | INTERFACE_MINUS_U_DOWN | — |
| a5 51 0d | INTERFACE_MINUS_U_UP | — |
| a5 50 05 | INTERFACE_PULSE_DOWN | — |
| a5 51 05 | INTERFACE_PULSE_UP | — |
| a5 50 11 | INTERFACE_SPEED_DOWN | — |
| a5 51 11 | INTERFACE_SPEED_UP | — |
| a5 50 06 | INTERFACE_PAUSE_DOWN | — |
| a5 51 06 | INTERFACE_PAUSE_UP | — |
| a5 50 09 | INTERFACE_STOP_DOWN | — |
| a5 51 09 | INTERFACE_STOP_UP | — |
| a5 50 5a | INTERFACE_RESET_DOWN | — |
| a5 51 5a | INTERFACE_RESET_UP | — |
| a5 50 0f | INTERFACE_TRACE_DOWN | — |
| a5 51 0f | INTERFACE_TRACE_UP | — |
| a5 50 07 | INTERFACE_ESCAPE_DOWN | — |
| a5 51 07 | INTERFACE_ESCAPE_UP | — |
| a5 50 12 | INTERFACE_LASER_GATE_DOWN | — |
| a5 51 12 | INTERFACE_LASER_GATE_UP | — |
| a5 50 08 | INTERFACE_ORIGIN_DOWN | — |
| a5 51 08 | INTERFACE_ORIGIN_UP | — |
| 80 00 | AXIS_X_MOVE | abscoord(x) |
| 80 01 | AXIS_Z_MOVE | abscoord(z) TODO: Should be 0x80 0x08? |
| 88 | MOVE_ABS_XY | abscoord(x), abscoord(y) |
| 89 | MOVE_REL_XY | relcoord(dx), relcoord(dy) |
| a0 00 | AXIS_A_MOVE | abscoord(a) TODO: Should be AXIS_Y_MOVE? |
| a0 08 | AXIS_U_MOVE | abscoord(u) |
| 8a | MOVE_REL_X | relcoord(dx) |
| 8b | MOVE_REL_Y | relcoord(dy) |
| a8 | CUT_ABS_XY | abscoord(x), abscoord(y) |
| a9 | CUT_REL_XY | relcoord(dx), relcoord(dy) |
| aa | CUT_REL_X | relcoord(dx) |
| ab | CUT_REL_Y | relcoord(dy) |
| c7 | IMD_POWER_1 | power(2) |
| c0 | IMD_POWER_2 | power(2) |
| c2 | IMD_POWER_3 | power(2) |
| c3 | IMD_POWER_4 | power(2) |
| c8 | END_POWER_1 | power(2) |
| c1 | END_POWER_2 | power(2) |
| c4 | END_POWER_3 | power(2) |
| c5 | END_POWER_4 | power(2) |
| c6 01 | MIN_POWER_1 | power(2) |
| c6 02 | MAX_POWER_1 | power(2) |
| c6 21 | MIN_POWER_2 | power(2) |
| c6 22 | MAX_POWER_2 | power(2) |
| c6 05 | MIN_POWER_3 | power(2) |
| c6 06 | MAX_POWER_3 | power(2) |
| c6 07 | MIN_POWER_4 | power(2) |
| c6 08 | MAX_POWER_4 | power(2) |
| c6 10 | LASER_INTERVAL | time(5) |
| c6 11 | ADD_DELAY | time(5) |
| c6 12 | LASER_ON_DELAY | time(5) |
| c6 13 | LASER_OFF_DELAY | time(5) |
| c6 15 | LASER_ON_DELAY2 | time(5) |
| c6 16 | LASER_OFF_DELAY2 | time(5) |
| c6 31 | MIN_POWER_1_PART | part(1), power(2) |
| c6 32 | MAX_POWER_1_PART | part(1), power(2) |
| c6 41 | MIN_POWER_2_PART | part(1), power(2) |
| c6 42 | MAX_POWER_2_PART | part(1), power(2) |
| c6 35 | MIN_POWER_3_PART | part(1), power(2) |
| c6 36 | MAX_POWER_3_PART | part(1), power(2 |
| c6 37 | MIN_POWER_4_PART | part(1), power(2) |
| c6 38 | MAX_POWER_4_PART | part(1), power(2) |
| c6 50 | THROUGH_POWER_1 | power(2) |
| c6 51 | THROUGH_POWER_2 | power(2) |
| c6 55 | THROUGH_POWER_3 | power(2) |
| c6 56 | THROUGH_POWER_4 | power(2) |
| c6 60 | FREQUENCY_PART | laser(1), part(1), frequency(5) |
| c9 02 | SPEED_LASER_1 | speed(5) |
| c9 03 | SPEED_AXIS | speed(5) |
| c9 04 | SPEED_LASER_1_PART | part(1), speed(5) |
| c9 05 | FORCE_ENG_SPEED | speed(5) |
| c9 06 | SPEED_AXIS_MOVE | speed(5) |
| ca 01 00 | LAYER_END | — |
| ca 01 01 | WORK_MODE_1 | — |
| ca 01 02 | WORK_MODE_2 | — |
| ca 01 03 | WORK_MODE_3 | — |
| ca 01 04 | WORK_MODE_4 | — |
| ca 01 55 | WORK_MODE_5 | — |
| ca 01 05 | WORK_MODE_6 | — |
| ca 01 10 | LASER_DEVICE_0 | — |
| ca 01 11 | LASER_DEVICE_1 | — |
| ca 01 12 | AIR_ASSIST_OFF | — |
| ca 01 13 | AIR_ASSIST_ON | — |
| ca 01 14 | DB_HEAD | — |
| ca 01 30 | EN_LASER_2_OFFSET_0 | — |
| ca 01 31 | EN_LASER_2_OFFSET_1 | — |
| ca 02 | LAYER_NUMBER_PART | part(1) |
| ca 03 | EN_LASER_TUBE_START | part(1) |
| ca 04 | X_SIGN_MAP | value(1) |
| ca 05 | LAYER_COLOR | color(5) |
| ca 06 | LAYER_COLOR_PART | part(1), color(5) |
| ca 10 | EN_EX_IO | value(1) |
| ca 22 | MAX_LAYER_PART | part(1) |
| ca 30 | U_FILE_ID | file_number(2) |
| ca 40 | ZU_MAP | value(1) |
| ca 41 | WORK_MODE_PART | part(1), mode(1) |
| cc | ACK | — |
| cf | NAK | — |
| cd | ERR | — |
| ce | ENQ | — |
| d7 | END_OF_FILE | — |
| d8 00 | START_PROCESS | — |
| d8 01 | STOP_PROCESS | — |
| d8 02 | PAUSE_PROCESS | — |
| d8 03 | RESTORE_PROCESS | — |
| d8 10 | REF_POINT_2 | MACHINE_ZERO/ABS POSITION |
| d8 11 | REF_POINT_1 | ANCHOR_POINT |
| d8 12 | REF_POINT_0 | CURRENT_POSITION |
| d8 2c | HOME_Z | — |
| d8 2d | HOME_U | — |
| d8 2a | HOME_XY | — |
| d8 2e | FOCUS_Z | — |
| d8 20 | KEYDOWN_X_LEFT | — |
| d8 21 | KEYDOWN_X_RIGHT | — |
| d8 22 | KEYDOWN_Y_TOP | — |
| d8 23 | KEYDOWN_Y_BOTTOM | — |
| d8 24 | KEYDOWN_Z_UP | — |
| d8 25 | KEYDOWN_Z_DOWN | — |
| d8 26 | KEYDOWN_U_FORWARD | — |
| d8 27 | KEYDOWN_U_BACKWARDS | — |
| d8 30 | KEYUP_X_LEFT | — |
| d8 31 | KEYUP_X_RIGHT | — |
| d8 32 | KEYUP_Y_TOP | — |
| d8 33 | KEYUP_Y_BOTTOM | — |
| d8 34 | KEYUP_Z_UP | — |
| d8 35 | KEYUP_Z_DOWN | — |
| d8 36 | KEYUP_U_FORWARD | — |
| d8 37 | KEYUP_U_BACKWARDS | — |
| d9 00 | RAPID_MOVE_X | options(1), abscoord(5) |
| d9 01 | RAPID_MOVE_Y | options(1), abscoord(5) |
| d9 02 | RAPID_MOVE_Z | options(1), abscoord(5) |
| d9 03 | RAPID_MOVE_U | options(1), abscoord(5) |
| d9 0f | RAPID_FEED_AXIS_MOVE | options(1) |
| d9 10 | RAPID_MOVE_XY | options(1), abscoord(5), abscoord(5) |
| d9 30 | RAPID_MOVE_XYU | options(1), abscoord(5), abscoord(5), abscoord(5) |
| da 00 | GET_SETTING | memory(2) |
| da 01 | SET_SETTING | memory(2), v0(5), v1(5) |
| e5 00 | DOCUMENT_FILE_UPLOAD | file_number(2), v0(5), v1(5) |
| e5 02 | DOCUMENT_FILE_END | — |
| e5 05 | SET_FILE_SUM | — |
| e6 01 | SET_ABSOLUTE | — |
| e7 00 | BLOCK_END | — |
| e7 01 | SET_FILENAME | filename (null terminated). |
| e7 03 | PROCESS_TOP_LEFT | abscoord(5), abscoord(5) |
| e7 04 | PROCESS_REPEAT | v0(2), v1(2), v2(2), v3(2), v4(2), v5(2), v6(2) |
| e7 05 | ARRAY_DIRECTION | direction(1) |
| e7 06 | FEED_REPEAT | v0(5), v1(5) |
| e7 07 | PROCESS_BOTTOM_RIGHT | abscoord(5), abscoord(5) |
| e7 08 | ARRAY_REPEAT | v0(2), v1(2), v2(2), v3(2), v4(2), v5(2), v6(2) |
| e7 09 | FEED_LENGTH | value(5) |
| e7 0a | FEED_INFO | — |
| e7 0b | ARRAY_EN_MIRROR_CUT | value(1) |
| e7 13 | ARRAY_MIN_POINT | abscoord(5), abscoord(5) |
| e7 17 | ARRAY_MAX_POINT | abscoord(5), abscoord(5) |
| e7 23 | ARRAY_ADD | abscoord(5), abscoord(5) |
| e7 24 | ARRAY_MIRROR | mirror(1) |
| e7 35 | BLOCK_X_SIZE | abscoord(5), abscoord(5) |
| e7 35 | BY_TEST | 0x11227766 TODO: Unclear why? |
| e7 37 | ARRAY_EVEN_DISTANCE | — |
| e7 38 | SET_FEED_AUTO_PAUSE | — |
| e7 3a | UNION_BLOCK_PROPERTY | — |
| e7 50 | DOCUMENT_MIN_POINT | abscoord(5), abscoord(5) |
| e7 51 | DOCUMENT_MAX_POINT | abscoord(5), abscoord(5) |
| e7 52 | PART_MIN_POINT | part(1), abscoord(5), abscoord(5) |
| e7 53 | PART_MAX_POINT | part(1), abscoord(5), abscoord(5) |
| e7 54 | PEN_OFFSET | axis(1), coord(5) |
| e7 55 | LAYER_OFFSET | axis(1), coord(5) |
| e7 60 | SET_CURRENT_ELEMENT_INDEX | index(1) |
| e7 61 | PART_MIN_POINT_EX | part(1), abscoord(5), abscoord(5) |
| e7 62 | PART_MAX_POINT_EX | part(1), abscoord(5), abscoord(5) |
| ea | ARRAY_START | index(1) |
| eb | ARRAY_END | — |
| f0 | REF_POINT_SET | — |
| f1 00 | ELEMENT_MAX_INDEX | index(1) |
| f1 01 | ELEMENT_NAME_MAX_INDEX | index(1) |
| f1 02 | ENABLE_BLOCK_CUTTING | enable(1) |
| f1 03 | DISPLAY_OFFSET | abscoord(5),  abscoord(5) |
| f1 04 | FEED_AUTO_CALC | enable(1) |
| f2 00 | ELEMENT_INDEX | index(1) |
| f2 02 | ELEMENT_NAME | name(10) |
| f2 03 | ELEMENT_ARRAY_MIN_POINT | abscoord(5),  abscoord(5) |
| f2 04 | ELEMENT_ARRAY_MAX_POINT | abscoord(5),  abscoord(5) |
| f2 05 | ELEMENT_ARRAY | v0(2), v1(2), v2(2), v3(2), v4(2), v5(2), v6(2) |
| f2 06 | ELEMENT_ARRAY_ADD | abscoord(5), abscoord(5) |
| f2 07 | ELEMENT_ARRAY_MIRROR | mirror(1) |

## Geheugenadressen

| Hex | Symbool |
|---|---|
| 05 7e | MEM_CARD_ID |
| 00 26 | MEM_BED_SIZE_X |
| 00 36 | MEM_BED_SIZE_Y |
| 04 00 | MEM_MACHINE_STATUS |
| 04 21 | MEM_CURRENT_X |
| 04 31 | MEM_CURRENT_Y |
| 04 41 | MEM_CURRENT_Z |
| 04 51 | MEM_CURRENT_U |
