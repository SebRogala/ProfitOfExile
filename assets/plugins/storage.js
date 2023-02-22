const storage = {
    install(app, options) {
        app.config.globalProperties.$storage = this;

        app.config.globalProperties.$storage.saveStrategy = (strategyName, strategy) => {
            let strategies = JSON.parse(localStorage.getItem('storedStrategies')) || {};

            let newStrategy = {};
            newStrategy[strategyName] = strategy;

            let merged = {...strategies, ...newStrategy}

            localStorage.setItem('storedStrategies', JSON.stringify(merged));
        };

        app.config.globalProperties.$storage.getStrategy = (strategyName) => {
            let strategies = JSON.parse(localStorage.getItem('storedStrategies'));

            if (null !== strategies && strategyName in strategies) {
                return strategies[strategyName];
            }

            return null;
        };
    },
};

export default storage;
