// Security: HTTPS, the company's directory (LDAP / Active Directory), single sign-on (OpenID Connect)
// and the audit journal.
// spacing: a no-break space (U+00A0) before ? ! : ; and inside « ».
export const security: Record<string, string> = {
  'Single sign-on failed:': 'Échec de l’authentification unique :',
  or: 'ou',
  'Sign in with {name}': 'Se connecter avec {name}',
  'Your company account (directory) works here too.': 'Votre compte d’entreprise (annuaire) fonctionne aussi ici.',
  'Administrator groups': 'Groupes administrateurs',
  'One a line: the group’s name, or its full DN (LDAP).': 'Un par ligne : le nom du groupe, ou son DN complet (LDAP).',
  'Designer groups': 'Groupes designers',
  'Operator groups': 'Groupes opérateurs',
  'The others': 'Les autres',
  'The role of someone in none of these groups.': 'Le rôle d’une personne qui n’est dans aucun de ces groupes.',
  Refused: 'Refusé',
  Administrator: 'Administrateur',
  'Encrypted interface': 'Interface chiffrée',
  'The web interface, the API and the output pages over HTTPS on the same port. Applied when the settings are saved (the engine restarts).':
    'L’interface web, l’API et les pages de sortie en HTTPS sur le même port. Appliqué à l’enregistrement des réglages (le moteur redémarre).',
  HTTPS: 'HTTPS',
  'Certificate (PEM)': 'Certificat (PEM)',
  'Path of the certificate (chain) on the server. Both empty: a certificate made for this machine, which browsers ask to trust once.':
    'Chemin du certificat (chaîne) sur le serveur. Les deux vides : un certificat créé pour cette machine, que les navigateurs demandent d’accepter une fois.',
  'Private key (PEM)': 'Clé privée (PEM)',
  'Plain HTTP from another machine is sent to HTTPS; the machine itself (127.0.0.1) stays reachable over HTTP for the menu bar app.':
    'Le HTTP venant d’une autre machine est redirigé vers HTTPS ; la machine elle-même (127.0.0.1) reste joignable en HTTP pour l’application de la barre des menus.',
  'Directory (LDAP / Active Directory)': 'Annuaire (LDAP / Active Directory)',
  'Sign in with the directory': 'Connexion par l’annuaire',
  Security: 'Sécurité',
  'Do not check its certificate': 'Ne pas vérifier son certificat',
  'Search account (DN)': 'Compte de recherche (DN)',
  'Empty: anonymous search.': 'Vide : recherche anonyme.',
  'Its password': 'Son mot de passe',
  'Kept on the server; leave empty to keep the saved one.': 'Conservé sur le serveur ; laisser vide pour garder celui enregistré.',
  'Base of the users (DN)': 'Base des utilisateurs (DN)',
  'User filter': 'Filtre utilisateur',
  '{user}: the name typed. Active Directory: (sAMAccountName={user}); OpenLDAP: (uid={user}).': '{user} : le nom saisi. Active Directory : (sAMAccountName={user}) ; OpenLDAP : (uid={user}).',
  'Name attribute': 'Attribut du nom',
  'An account of this server signs in first; the others are checked against the directory, their account made on first sign-in with the role of their groups (brought up to date at each sign-in).':
    'Un compte de ce serveur est essayé d’abord ; les autres sont vérifiés auprès de l’annuaire, leur compte est créé à la première connexion avec le rôle de leurs groupes (mis à jour à chaque connexion).',
  'Single sign-on (OpenID Connect)': 'Authentification unique (OpenID Connect)',
  'Sign-in button': 'Bouton de connexion',
  'Words of the button': 'Texte du bouton',
  Issuer: 'Émetteur (issuer)',
  'Client ID': 'ID client',
  'Client secret': 'Secret client',
  'Name claim': 'Claim du nom',
  'Groups claim': 'Claim des groupes',
  'Address to give the provider (redirect URI):': 'Adresse à donner au fournisseur (URI de redirection) :',
  'Audit journal': 'Journal d’audit',
  'Every change made through the interface or the API: sign-ins, settings, multiviews, alarms, users…':
    'Chaque modification faite par l’interface ou l’API : connexions, réglages, multiviews, alarmes, utilisateurs…',
  User: 'Utilisateur',
  Action: 'Action',
  Result: 'Résultat',
  // Settings › Integrations: NMOS and Ember+.
  'Controllers and registries of the IP plant see the server: its ST 2110 outputs as senders, its UDP / RTP inputs as receivers.':
    'Les contrôleurs et registres de l’installation IP voient le serveur : ses sorties ST 2110 comme émetteurs (senders), ses entrées UDP / RTP comme récepteurs (receivers).',
  Registry: 'Registre',
  'Its address (IS-04 registration API). Empty: no registration, the Node API is still served for direct control.':
    'Son adresse (API d’enregistrement IS-04). Vide : pas d’enregistrement, l’API Node reste servie pour un contrôle direct.',
  'Address announced': 'Adresse annoncée',
  'The address controllers reach the server at. Empty: the first one of the machine.': 'L’adresse par laquelle les contrôleurs joignent le serveur. Vide : la première de la machine.',
  'Node {id}: {s} sender(s), {r} receiver(s).': 'Nœud {id} : {s} émetteur(s), {r} récepteur(s).',
  'Activating a sender changes its destination or turns it on or off; a receiver, the multicast group its input joins. Both are saved in the settings. The NMOS APIs are open (no key), as is usual on a media network.':
    'Activer un émetteur change sa destination ou l’allume / l’éteint ; un récepteur, le groupe multicast que rejoint son entrée. Les deux sont enregistrés dans les réglages. Les API NMOS sont ouvertes (sans clé), comme d’usage sur un réseau média.',
  'Control panels and control systems (VSM, Lawo, Cerebrum…) browse the server: multiview on air of each channel, state and faults of each input, acknowledgement.':
    'Les panneaux et systèmes de contrôle (VSM, Lawo, Cerebrum…) parcourent le serveur : multiview à l’antenne de chaque chaîne, état et alarmes de chaque entrée, acquittement.',
  'TCP port': 'Port TCP',
  'Tree: strata › info (name, version, uptime, faults, acknowledgeAll), channels › <channel> › multiview, inputs › <input> › live, status, alarms, loudness.':
    'Arbre : strata › info (name, version, uptime, alarms, acknowledgeAll), channels › <chaîne> › multiview, inputs › <entrée> › live, status, alarms, loudness.',
  Integrations: 'Intégrations',
  // Single sign-on: providers, Okta.
  'Other (OpenID Connect)': 'Autre (OpenID Connect)',
  'Okta: your org (https://<org>.okta.com), or an authorization server of yours (https://<org>.okta.com/oauth2/default).':
    'Okta : votre organisation (https://<org>.okta.com), ou l’un de vos serveurs d’autorisation (https://<org>.okta.com/oauth2/default).',
  'The address the provider gives as « issuer ».': 'L’adresse que le fournisseur indique comme « issuer ».',
  Scopes: 'Scopes',
  'openid is always asked for. Okta: groups gives the groups with the org authorization server; with oauth2/default, remove it and add a groups claim on the authorization server instead.':
    'openid est toujours demandé. Okta : groups donne les groupes avec le serveur d’autorisation de l’organisation ; avec oauth2/default, retirez-le et ajoutez plutôt un claim groups sur le serveur d’autorisation.',
  'Okta admin console › Applications › Create App Integration: OIDC - OpenID Connect, Web Application.':
    'Console d’administration Okta › Applications › Create App Integration : OIDC - OpenID Connect, Web Application.',
  'Sign-in redirect URI: the address above. Sign-out redirect URI: the address of this server.':
    'Sign-in redirect URI : l’adresse ci-dessus. Sign-out redirect URI : l’adresse de ce serveur.',
  'Assignments: the groups allowed to sign in. Copy the Client ID and the Client secret here.':
    'Assignments : les groupes autorisés à se connecter. Copiez ici le Client ID et le Client secret.',
  'Sign On tab › OpenID Connect ID Token › Groups claim: type Filter, name groups, Matches regex .* (or Starts with mv-).':
    'Onglet Sign On › OpenID Connect ID Token › Groups claim : type Filter, nom groups, Matches regex .* (ou Starts with mv-).',
  'Name the Okta groups in the lists above (e.g. mv-admins): their members get that role at each sign-in.':
    'Indiquez les groupes Okta dans les listes ci-dessus (par ex. mv-admins) : leurs membres reçoivent ce rôle à chaque connexion.',
};
