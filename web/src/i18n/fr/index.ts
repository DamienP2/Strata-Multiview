// French texts, by English wording. One file per part of the interface.
//
// Words: template → modèle, layer → calque, rundown → conducteur, channel →
// canal, on air → à l’antenne, data set → jeu de données, lower third →
// synthé, asset → ressource. Buttons of the transport (TAKE, NEXT, OUT,
// CUE, UPDATE, CONTINUE, PVW, PGM) keep their broadcast names. French
// spacing: a no-break space (U+00A0) before ? ! : ; and inside « ».
import { asrun } from './asrun';
import { buttons } from './buttons';
import { common } from './common';
import { controls } from './controls';
import { ccA } from './ccA';
import { ccB } from './ccB';
import { ccC } from './ccC';
import { ccD } from './ccD';
import { data } from './data';
import { dataRules } from './dataRules';
import { designer } from './designer';
import { home } from './home';
import { importer } from './importer';
import { operator } from './operator';
import { operatorLocks } from './operatorLocks';
import { schedule } from './schedule';
import { shortcuts } from './shortcuts';
import { social } from './social';
import { socialSources } from './socialSources';
import { socialWidgets } from './socialWidgets';
import { widgets } from './widgets';
import { widgetsData } from './widgetsData';
import { widgetsA } from './widgetsA';
import { widgetsB } from './widgetsB';
import { widgetsC } from './widgetsC';
import { ux3 } from './ux3';
import { ux1 } from './ux1';
import { ux2 } from './ux2';

export const fr: Record<string, string> = { ...common, ...controls, ...ccA, ...ccB, ...ccC, ...ccD, ...home, ...importer, ...designer, ...data, ...dataRules, ...operator, ...operatorLocks, ...schedule, ...shortcuts, ...widgets, ...widgetsData, ...widgetsA, ...widgetsB, ...widgetsC, ...asrun, ...buttons, ...socialSources, ...socialWidgets, ...social, ...ux3, ...ux1, ...ux2 };
