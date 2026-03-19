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
			components: {
				MarkdownContent: './src/components/MarkdownContent.astro',
			},
			sidebar: [
				{
					label: 'Guides',
					items: [
						{ label: 'Getting Started', slug: 'guides/getting-started' },
						{ label: 'Configuration', slug: 'guides/configuration' },
						{
							label: 'Plugins',
							items: [
								{ label: 'Overview', slug: 'guides/plugins' },
								{ label: 'Yazi', slug: 'guides/plugins/yazi' },
							],
						},
					],
				},
				{
					label: 'Contributing',
					items: [
						{ label: 'Local Development', slug: 'contributing/local-development' },
						{ label: 'Testing Strategy', slug: 'contributing/testing-strategy' },
						{ label: 'How To Add A Command', slug: 'contributing/how-to-add-a-command' },
					],
				},
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
