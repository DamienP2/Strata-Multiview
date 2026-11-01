// Operations: tile markers, input backups, loudness logs, incidents, salvos, audio monitoring, walls, latency.
// spacing: a no-break space (U+00A0) before ? ! : ; and inside « ».
export const ops: Record<string, string> = {
  Markers: 'Repères',
  'Action safe (93 %)': 'Zone de sécurité action (93 %)',
  'Graphics safe (90 %)': 'Zone de sécurité titres (90 %)',
  '4:3 frame': 'Cadre 4:3',
  '14:9 frame': 'Cadre 14:9',
  'AFD: the active area the picture signals, and its code': 'AFD : la zone active signalée par l’image, et son code',
  'On backup': 'Sur secours',
  'The input lost its signal: its tiles show its backup input.': 'L’entrée a perdu son signal : ses tuiles montrent son entrée de secours.',
  'Backup input': 'Entrée de secours',
  'Shown in its tiles when this input loses its signal: picture, sound and faults of the backup, with the name and tally of this input. The alarm « On backup » is raised.':
    'Montrée dans ses tuiles quand cette entrée perd son signal : image, son et alarmes du secours, avec le nom et le tally de cette entrée. L’alarme « Sur secours » se lève.',
  'Switch after': 'Basculer après',
  'Seconds without a picture before its backup is shown.': 'Secondes sans image avant de montrer le secours.',
  'Come back after': 'Revenir après',
  'Seconds of steady pictures before this input is shown again; 0: it stays on its backup until switched back by hand.':
    'Secondes d’image stable avant de remontrer cette entrée ; 0 : elle reste sur le secours jusqu’au retour manuel.',
  'Show now': 'Montrer maintenant',
  'This input': 'Cette entrée',
  'Its backup': 'Son secours',
};
