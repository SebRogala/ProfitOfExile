/*
 * Welcome to your app's main JavaScript file!
 *
 * We recommend including the built version of this JavaScript file
 * (and its CSS file) in your base layout (base.html.twig).
 */

// any CSS you import will output into a single css file (app.css in this case)
import './styles/app.css';

import Vue from './App'
import {createApp} from 'vue';
import router from "./router";

import vuetify from './plugins/vuetify'
import api from "./plugins/api";
import datetime from "./plugins/datetime";
import storage from "./plugins/storage"
import filters from "./plugins/filters";

createApp(Vue)
  .use(router)
  .use(vuetify)
  .use(api)
  .use(datetime)
  .use(storage)
  .use(filters)
  .mount('#app');
