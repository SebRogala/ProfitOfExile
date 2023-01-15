import axios from 'axios'

const api = {
    install(app, options) {

        // axios.defaults.baseURL = process.env.VUE_APP_BASE
        axios.defaults.headers['Content-Type'] = 'application/json'

        // Potential preparation for excluding front as full SPA
        /*this.$axios.interceptors.request.use(req => {
          req.headers['Authorization'] = `Bearer ${accessToken}`
          return req;
        });*/


        app.config.globalProperties.$api = axios;

        app.config.globalProperties.$api.formData = (data) => {
            let formData = new FormData();

            Object.keys(data).forEach(key => {
                formData.append(key, data[key]);
            });

            return formData;
        };
    }
};

export default api;
