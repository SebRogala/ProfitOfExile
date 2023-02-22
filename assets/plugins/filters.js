const filters = {
    install(app, options) {
        app.config.globalProperties.$filters = {
            number(value) {
                return Number(value).toLocaleString()
            }
        };
    },
};

export default filters;
