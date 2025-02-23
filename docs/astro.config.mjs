// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
	site: 'https://kolloch.github.io',
	base: 'zack',
	integrations: [
		starlight({
			title: 'Zack Build',
			editLink: {
				baseUrl: 'https://github.com/kolloch/zack/edit/main/docs/',
			},
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
