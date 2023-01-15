const datetime = {
    install(app, options) {
        app.config.globalProperties.$datetime = arg => {
            return new Date(arg).toLocaleString("pl-PL", {
                year: "numeric",
                month: "2-digit",
                day: "2-digit",
                hour: "numeric",
                minute: "numeric",
                second: "numeric",
            });
        }

    }
};

export default datetime;
