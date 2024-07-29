/** @type {import('tailwindcss').Config} */
export default {
	content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
	theme: {
		extend: {
			keyframes: {
				overlay: {
					from: { opacity: '0' },
					to: { opacity: '1' },
				},
				content: {
					from: { opacity: '0', transform: 'translate(-50%, -48%) scale(0.96)' },
					to: { opacity: '1', transform: 'translate(-50%, -50%) scale(1)' },
				},
			},
			animation: {
				overlay: 'overlay 150ms cubic-bezier(0.16, 1, 0.3, 1)',
				content: 'content 150ms cubic-bezier(0.16, 1, 0.3, 1)',
			},
		},
	},
	plugins: [],
};
