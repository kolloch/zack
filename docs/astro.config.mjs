// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import starlightLinksValidator from 'starlight-links-validator'

import markdoc from '@astrojs/markdoc';

// https://astro.build/config
export default defineConfig({
    site: 'https://kolloch.github.io',
    base: 'zack',
    i18n: {
        routing: {
            redirectToDefaultLocale: undefined,
        }
    },
    integrations: [starlight({
        plugins: [starlightLinksValidator()],
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
                label: 'About Zack',
                autogenerate: { directory: '00_zack' },
            },
            {
                label: 'Concepts',
                collapsed: true,
                autogenerate: { directory: 'concepts' },
            },
            {
                label: 'Low-level Components',
                collapsed: true,
                autogenerate: { directory: 'components' },
            },
            {
                label: 'Other Build Systems',
                collapsed: true,
                autogenerate: { directory: 'other_build_systems' },
            },
        ],
		}), markdoc()],
});
