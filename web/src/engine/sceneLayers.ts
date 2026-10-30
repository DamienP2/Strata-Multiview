import { createContext } from 'react';
import type { Layer } from './types';

/** The layers of the graphic a widget is drawn in (set by Scene): a widget can follow another one. */
export const SceneLayers = createContext<Layer[]>([]);
