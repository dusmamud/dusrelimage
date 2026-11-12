// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import sitemap from '@astrojs/sitemap';

const jsonLd = {
	'@context': 'https://schema.org',
	'@type': 'SoftwareApplication',
	name: 'dusrelimage',
	applicationCategory: 'DeveloperApplication',
	operatingSystem: 'Cross-platform',
	description:
		'Offline image toolkit — compress, resize, crop, convert, watermark, annotate, pick colours, extract palettes, build collages and pixelate. One Rust core, zero network calls.',
	author: { '@type': 'Person', name: 'dusmamud' },
	offers: { '@type': 'Offer', price: '0', priceCurrency: 'INR' },
	softwareVersion: '0.1.0',
	url: 'https://dusrelimage.pages.dev/',
	downloadUrl: 'https://www.npmjs.com/package/dusrelimage',
	programmingLanguage: ['Rust', 'JavaScript', 'TypeScript', 'Python', 'Go', 'PHP', 'Kotlin', 'Swift', 'C'],
	license: 'https://github.com/dusmamud/dusrelimage/blob/main/LICENSE',
};

// https://astro.build/config
export default defineConfig({
	site: 'https://dusrelimage.pages.dev',
	integrations: [
		starlight({
			title: 'dusrelimage',
			description:
				'Offline image toolkit — compress, resize, crop, convert, watermark, annotate, pick colours, extract palettes, build collages and pixelate. Rust core, zero network calls.',
			logo: {
				src: './src/assets/logo.svg',
				alt: 'dusrelimage logo',
			},
			favicon: '/favicon.svg',
			social: [
				{ icon: 'github', label: 'GitHub', href: 'https://github.com/dusmamud/dusrelimage' },
			],
			head: [
				{ tag: 'meta', attrs: { property: 'og:type', content: 'website' } },
				{ tag: 'meta', attrs: { property: 'og:site_name', content: 'dusrelimage' } },
				{ tag: 'meta', attrs: { property: 'og:title', content: 'dusrelimage — Offline image toolkit' } },
				{
					tag: 'meta',
					attrs: {
						property: 'og:description',
						content:
							'Compress, resize, crop, convert, watermark, annotate, pick colours, extract palettes, build collages and pixelate — offline. One Rust core, bindings for JavaScript, Python, Rust, Go, PHP, Android and Swift, plus a CLI.',
					},
				},
				{ tag: 'meta', attrs: { property: 'og:url', content: 'https://dusrelimage.pages.dev/' } },
				{ tag: 'meta', attrs: { property: 'og:image', content: 'https://dusrelimage.pages.dev/og.svg' } },
				{ tag: 'meta', attrs: { name: 'twitter:card', content: 'summary_large_image' } },
				{ tag: 'meta', attrs: { name: 'twitter:title', content: 'dusrelimage — Offline image toolkit' } },
				{
					tag: 'meta',
					attrs: {
						name: 'twitter:description',
						content:
							'Compress, resize, crop, convert, watermark, annotate, pick colours, extract palettes, build collages and pixelate — offline. One Rust core, JavaScript, Python, Rust, Go, PHP, Android and Swift bindings, plus a CLI.',
					},
				},
				{ tag: 'meta', attrs: { name: 'twitter:image', content: 'https://dusrelimage.pages.dev/og.svg' } },
				{
					tag: 'script',
					attrs: { type: 'application/ld+json' },
					content: JSON.stringify(jsonLd),
				},
			],
			editLink: {
				baseUrl: 'https://github.com/dusmamud/dusrelimage/edit/main/website/',
			},
			sidebar: [
				{
					label: 'Start here',
					items: [
						{ label: 'Overview', slug: 'overview' },
						{ label: 'Quickstart', slug: 'guides/quickstart' },
						{ label: 'Options', slug: 'guides/options' },
						{ label: 'Error codes', slug: 'guides/error-codes' },
					],
				},
				{
					label: 'Languages',
					items: [
						{ label: 'C API', slug: 'guides/c-api' },
						{ label: 'Go', slug: 'guides/go' },
						{ label: 'PHP', slug: 'guides/php' },
						{ label: 'Android', slug: 'guides/android' },
						{ label: 'Swift', slug: 'guides/swift' },
					],
				},
				{
					label: 'Tools',
					items: [
						{ label: 'Compress', slug: 'reference/compress' },
						{ label: 'Resize', slug: 'reference/resize' },
						{ label: 'Crop', slug: 'reference/crop' },
						{ label: 'Convert', slug: 'reference/convert' },
						{ label: 'Watermark', slug: 'reference/watermark' },
						{ label: 'Annotate', slug: 'reference/annotate' },
						{ label: 'Colour picker', slug: 'reference/colour-picker' },
						{ label: 'Palette', slug: 'reference/palette' },
						{ label: 'Collage', slug: 'reference/collage' },
						{ label: 'Pixelate', slug: 'reference/pixelate' },
						{ label: 'Info & bulk', slug: 'reference/info-and-bulk' },
					],
				},
				{
					label: 'Reference',
					items: [
						{ label: 'Comparison', slug: 'comparison' },
						{ label: 'Changelog', slug: 'changelog' },
					],
				},
			],
		}),
		sitemap(),
	],
});
