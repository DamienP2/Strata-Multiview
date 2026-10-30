// The Operator with several operators: people on the channel, zones held, undo.
export const operatorLocks: Record<string, string> = {
  // People on the channel
  '{name} (you)': '{name} (vous)',
  'you': 'vous',
  // Zones held
  '{name} took the zone {zone}': '{name} a pris la zone {zone}',
  'Free the zone {zone}': 'Libérer la zone {zone}',
  'Take the zone {zone} from {name}': 'Prendre la zone {zone} à {name}',
  'Take the zone {zone}': 'Prendre la zone {zone}',
  'free': 'libre',
  '{name} holds this zone': '{name} tient cette zone',
  // Undo
  'undo|Clear all': 'Tout effacer',
  'undo|Row': 'Ligne',
  'undo|Widget': 'Widget',
  'undo|Trigger': 'Déclencheur',
  'Nothing to undo': 'Rien à annuler',
  'Undone: {name}': 'Annulé : {name}',
  'Last actions on air': 'Dernières actions à l’antenne',
  'Undo back to here': 'Annuler jusqu’ici',
  'Undo {n} action': 'Annuler {n} action',
  'Undo {n} actions': 'Annuler {n} actions',
};
