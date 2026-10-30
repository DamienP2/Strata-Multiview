// The control centre of the widgets (web/src/widgets/centre.tsx and the
// widgets' three lines): the state, the actions, the settings. The actions
// use the `cc` context (tc): their wording is the centre's, whatever the
// same English word means elsewhere.
export const controls: Record<string, string> = {
  'Next in {n} s': 'Suivante dans {n} s',
  'Waits for Continue': 'Attend Continuer',
  'Waits for the trigger “{name}”': 'Attend le déclencheur « {name} »',
  'cc|trigger': 'déclencheur',
  'cc|Set': 'Régler',
  // Start / advance modes.
  'cc|At the In': 'Dès le IN',
  'cc|Continue F2': 'Continuer F2',
  'cc|Trigger': 'Déclencheur',
  'cc|Timed': 'Minutée',
  'cc|Start': 'Démarrer',
  'cc|Trigger name': 'Nom du déclencheur',
  // Actions.
  'cc|Play': 'Lire',
  'cc|Pause': 'Pause',
  'cc|Stop': 'Arrêt',
  'cc|Continue': 'Continuer',
  'cc|Previous': 'Précédente',
  'cc|Next': 'Suivante',
  'cc|Refresh': 'Actualiser',
  'cc|Reset': 'Réinitialiser',
  'cc|Restart': 'Relancer',
  'cc|Fire the trigger': 'Déclencher',
  // The table.
  'cc|Page': 'Page',
  'Chain the pages': 'Enchaîner les pages',
  'Next page': 'Page suivante',
  'Time per page': 'Durée par page',
  'cc|Highlighted row': 'Ligne en surbrillance',
  'Refresh the data': 'Actualiser les données',
  'cc|Go to page': 'Aller à la page',
  'How it starts on air: at the In, on Continue (F2), or on a trigger (also the API action widget)': 'Comment il démarre à l’antenne\u00a0: dès le IN, sur Continuer (F2) ou sur un déclencheur (aussi l’action widget de l’API)',
};
