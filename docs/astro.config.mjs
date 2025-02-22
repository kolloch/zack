// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
	integrations: [
		starlight({
			title: 'Zack',
			logo: {
				src: "./public/favicon.svg"
			},
			social: {
				github: 'https://github.com/kolloch/zack',
			},
			sidebar: [
				{
					label: 'Ideas',
					autogenerate: { directory: 'ideas' },
				},
				{
					label: 'Reference',
					autogenerate: { directory: 'reference' },
				},
			],
		}),
	],
});
