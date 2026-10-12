import React from 'react';
import { createRoot } from 'react-dom/client';
import { CasinoApp } from '@commit-casino/game-ui';
import { desktopService, desktopIntegration } from './platform';
createRoot(document.getElementById('root')!).render(<React.StrictMode><CasinoApp service={desktopService} integration={desktopIntegration}/></React.StrictMode>);
