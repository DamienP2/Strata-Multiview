// French texts, by English wording. One file per part of the interface.
//
// Words: template → modèle, layer → calque, rundown → conducteur, channel →
// canal, on air → à l’antenne, data set → jeu de données, lower third →
// synthé, asset → ressource. Buttons of the transport (TAKE, NEXT, OUT,
// CUE, UPDATE, CONTINUE, PVW, PGM) keep their broadcast names. French
// spacing: a no-break space (U+00A0) before ? ! : ; and inside « ».
import { buttons } from './buttons';
import { common } from './common';
import { controls } from './controls';
import { ccA } from './ccA';
import { ccB } from './ccB';
import { ccC } from './ccC';
import { ccD } from './ccD';
import { designer } from './designer';
import { home } from './home';
import { widgets } from './widgets';
import { widgetsA } from './widgetsA';
import { widgetsB } from './widgetsB';
import { widgetsC } from './widgetsC';
import { ux3 } from './ux3';
import { ux1 } from './ux1';
import { ux2 } from './ux2';
import { shared } from './shared';

export const fr: Record<string, string> = { ...common, ...controls, ...ccA, ...ccB, ...ccC, ...ccD, ...home, ...designer, ...widgets, ...widgetsA, ...widgetsB, ...widgetsC, ...buttons, ...ux3, ...ux1, ...ux2, ...shared };
