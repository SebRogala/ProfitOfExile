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

        app.config.globalProperties.$storage.getStrategyNames = () => {
            let strategies = JSON.parse(localStorage.getItem('storedStrategies'));

            if (null === strategies) {
                return [];
            }

            return Object.keys(strategies);
        };

        app.config.globalProperties.$storage.deleteStrategy = (strategyName) => {
            let strategies = JSON.parse(localStorage.getItem('storedStrategies')) || {};

            delete strategies[strategyName];

            localStorage.setItem('storedStrategies', JSON.stringify(strategies));
        };

    },
};

export default storage;
