// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

const isGitHubActions = process.env.GITHUB_ACTIONS === 'true';

export default defineConfig({
	site: 'https://zooeywm.github.io',
	base: isGitHubActions ? '/rim' : '/',
	integrations: [
		starlight({
			title: 'rim Docs',
			social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/zooeywm/rim' }],
			// Keep the sidebar explicit so the old README sections map to stable doc URLs.
			customCss: ['./src/styles/custom.css'],
			sidebar: [
				{
					label: 'Guides',
					items: [
						{ label: 'Getting Started', slug: 'guides/getting-started' },
						{ label: 'Configuration', slug: 'guides/configuration' },
						{ label: 'Editor Workflows', slug: 'guides/editor-workflows' },
						{ label: 'Runtime And Recovery', slug: 'guides/runtime-and-recovery' },
					],
				},
				{
					label: 'Tutorials',
					items: [
						{ label: 'Interactive Tutorials', slug: 'tutorials' },
						{ label: 'Mode Transitions', slug: 'tutorials/mode-transitions' },
						{ label: 'Key Hints', slug: 'tutorials/key-hints' },
						{ label: 'Command Palette', slug: 'tutorials/command-palette' },
						{ label: 'Workspace File Picker', slug: 'tutorials/workspace-file-picker' },
						{ label: 'Notification Center', slug: 'tutorials/notification-center' },
						{ label: 'Windows And Tabs', slug: 'tutorials/windows-and-tabs' },
						{ label: 'Visual Selection', slug: 'tutorials/visual-selection' },
						{ label: 'Visual Block Insert', slug: 'tutorials/visual-block-insert' },
					],
				},
				{
					label: 'Reference',
					items: [
						{ label: 'Features', slug: 'reference/features' },
						{ label: 'Workspace Layout', slug: 'reference/workspace-layout' },
						{ label: 'Architecture', slug: 'reference/architecture' },
						{ label: 'Commands', slug: 'reference/commands' },
					],
				},
			],
		}),
	],
});
