// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
	site: 'https://zooeywm.github.io',
	base: '/rim/',
	integrations: [
		starlight({
			title: 'rim Docs',
			social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/zooeywm/rim' }],
			customCss: ['./src/styles/custom.css'],
			sidebar: [
				{
					label: 'Architecture',
					items: [
						{ label: 'Overview', slug: 'architecture/overview' },
						{ label: 'Domain Model', slug: 'architecture/domain-model' },
						{ label: 'Application Layer', slug: 'architecture/application-layer' },
						{ label: 'Ports And Adapters', slug: 'architecture/ports-and-adapters' },
						{ label: 'Runtime Event Loop', slug: 'architecture/runtime-event-loop' },
						{ label: 'Dependency Rules', slug: 'architecture/dependency-rules' },
					],
				},
				{
					label: 'Guides',
					items: [
						{ label: 'Getting Started', slug: 'guides/getting-started' },
						{ label: 'Local Development', slug: 'guides/local-development' },
						{ label: 'Testing Strategy', slug: 'guides/testing-strategy' },
						{ label: 'How To Add A Command', slug: 'guides/how-to-add-a-command' },
					],
				},
				{
					label: 'Reference',
					items: [
						{ label: 'Config', slug: 'reference/config' },
						{ label: 'Workspace Session', slug: 'reference/workspace-session' },
						{ label: 'Undo Redo Swap', slug: 'reference/undo-redo-swap' },
					],
				},
			],
		}),
	],
});
